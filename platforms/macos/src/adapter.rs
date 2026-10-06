// Copyright 2022 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

use crate::{
    context::{ActionHandlerNoMut, ActionHandlerWrapper, Context, NativeChildren},
    event::{EventGenerator, QueuedEvents, focus_event},
    filters::filter,
    node::can_be_focused,
    util::*,
};
use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, Node as NodeProvider, NodeId as LocalNodeId,
    Role, Tree as TreeData, TreeId, TreeUpdate,
};
use accesskit_consumer::{FilterResult, Tree};
use hashbrown::HashMap;
use objc2::{
    msg_send, msg_send_id,
    rc::{Id, WeakId},
    sel,
};
use objc2_app_kit::NSView;
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect};
use std::fmt::{Debug, Formatter};
use std::{ffi::c_void, ptr::null_mut, rc::Rc};

const PLACEHOLDER_ROOT_ID: LocalNodeId = LocalNodeId(0);

enum State {
    Inactive {
        view: WeakId<NSView>,
        is_view_focused: bool,
        action_handler: Rc<dyn ActionHandlerNoMut>,
        mtm: MainThreadMarker,
    },
    Placeholder {
        placeholder_context: Rc<Context>,
        is_view_focused: bool,
        action_handler: Rc<dyn ActionHandlerNoMut>,
    },
    Active(Rc<Context>),
}

impl Debug for State {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            State::Inactive {
                view,
                is_view_focused,
                action_handler: _,
                mtm,
            } => f
                .debug_struct("Inactive")
                .field("view", view)
                .field("is_view_focused", is_view_focused)
                .field("mtm", mtm)
                .finish(),
            State::Placeholder {
                placeholder_context,
                is_view_focused,
                action_handler: _,
            } => f
                .debug_struct("Placeholder")
                .field("placeholder_context", placeholder_context)
                .field("is_view_focused", is_view_focused)
                .finish(),
            State::Active(context) => f.debug_struct("Active").field("context", context).finish(),
        }
    }
}

struct PlaceholderActionHandler;

impl ActionHandler for PlaceholderActionHandler {
    fn do_action(&mut self, _request: ActionRequest) {}
}

#[derive(Debug)]
pub struct Adapter {
    state: State,
    native_children: NativeChildren,
}

impl Adapter {
    /// Create a new macOS adapter. This function must be called on
    /// the main thread.
    ///
    /// The action handler will always be called on the main thread.
    ///
    /// # Safety
    ///
    /// `view` must be a valid, unreleased pointer to an `NSView`.
    pub unsafe fn new(
        view: *mut c_void,
        is_view_focused: bool,
        action_handler: impl 'static + ActionHandler,
    ) -> Self {
        let view = unsafe { Id::retain(view as *mut NSView) }.unwrap();
        let view = WeakId::from_id(&view);
        let mtm = MainThreadMarker::new().unwrap();
        let state = State::Inactive {
            view,
            is_view_focused,
            action_handler: Rc::new(ActionHandlerWrapper::new(action_handler)),
            mtm,
        };
        Self {
            state,
            native_children: NativeChildren::default(),
        }
    }

    /// Presents host-owned NSAccessibility elements as the trailing children of nodes in the
    /// root tree, replacing every previous attachment. Nodes absent from `children` lose theirs.
    ///
    /// The host keeps implementing each element's attributes and posts its notifications.
    /// The adapter sets each element's `accessibilityParent` to its node while the tree is
    /// active, returns elements from hit testing within their frames, and reports an element
    /// as focused when its node holds focus and the element answers `isAccessibilityFocused`.
    /// Elements attached to a node that the tree filters out are not presented.
    ///
    /// # Safety
    ///
    /// Every pointer must be a valid, unreleased `NSObject` implementing the
    /// `NSAccessibility` protocol. The adapter retains each element until it is replaced.
    pub unsafe fn set_native_children(
        &mut self,
        children: impl IntoIterator<Item = (LocalNodeId, Vec<*mut c_void>)>,
    ) {
        let children = children
            .into_iter()
            .filter_map(|(node, elements)| {
                let elements = elements
                    .into_iter()
                    .filter_map(|element| unsafe { Id::retain(element as *mut NSObject) })
                    .collect::<Vec<_>>();
                (!elements.is_empty()).then_some((node, elements))
            })
            .collect::<HashMap<_, _>>();
        if let State::Active(context) = &self.state {
            let tree = context.tree.borrow();
            let state = tree.state();
            for (node, elements) in &children {
                let Some(node) = state.node_by_tree_local_id(*node, TreeId::ROOT) else {
                    continue;
                };
                let parent = context.get_or_create_platform_node(node.id());
                for element in elements {
                    if element.respondsToSelector(sel!(setAccessibilityParent:)) {
                        let _: () = unsafe { msg_send![element, setAccessibilityParent: &*parent] };
                    }
                }
            }
        }
        *self.native_children.borrow_mut() = children;
    }

    /// If and only if the tree has been initialized, call the provided function
    /// and apply the resulting update. Note: If the caller's implementation of
    /// [`ActivationHandler::request_initial_tree`] initially returned `None`,
    /// the [`TreeUpdate`] returned by the provided function must contain
    /// a full tree.
    ///
    /// If a [`QueuedEvents`] instance is returned, the caller must call
    /// [`QueuedEvents::raise`] on it.
    pub fn update_if_active(
        &mut self,
        update_factory: impl FnOnce() -> TreeUpdate,
    ) -> Option<QueuedEvents> {
        match &self.state {
            State::Inactive { .. } => None,
            State::Placeholder {
                placeholder_context,
                is_view_focused,
                action_handler,
            } => {
                let tree = Tree::new(update_factory(), *is_view_focused);
                let context = Context::new(
                    placeholder_context.view.clone(),
                    tree,
                    Rc::clone(action_handler),
                    Rc::clone(&self.native_children),
                    placeholder_context.mtm,
                );
                let result = context.tree.borrow().state().focus().map(|node| {
                    QueuedEvents::new(Rc::clone(&context), vec![focus_event(node.id())])
                });
                self.state = State::Active(context);
                result
            }
            State::Active(context) => {
                let mut event_generator = EventGenerator::new(context.clone());
                let mut tree = context.tree.borrow_mut();
                tree.update_and_process_changes(update_factory(), &mut event_generator);
                Some(event_generator.into_result())
            }
        }
    }

    /// Update the tree state based on whether the window is focused.
    ///
    /// If a [`QueuedEvents`] instance is returned, the caller must call
    /// [`QueuedEvents::raise`] on it.
    pub fn update_view_focus_state(&mut self, is_focused: bool) -> Option<QueuedEvents> {
        match &mut self.state {
            State::Inactive {
                is_view_focused, ..
            } => {
                *is_view_focused = is_focused;
                None
            }
            State::Placeholder {
                is_view_focused, ..
            } => {
                *is_view_focused = is_focused;
                None
            }
            State::Active(context) => {
                let mut event_generator = EventGenerator::new(context.clone());
                let mut tree = context.tree.borrow_mut();
                tree.update_host_focus_state_and_process_changes(is_focused, &mut event_generator);
                Some(event_generator.into_result())
            }
        }
    }

    fn get_or_init_context<H: ActivationHandler + ?Sized>(
        &mut self,
        activation_handler: &mut H,
    ) -> Rc<Context> {
        match &self.state {
            State::Inactive {
                view,
                is_view_focused,
                action_handler,
                mtm,
            } => match activation_handler.request_initial_tree() {
                Some(initial_state) => {
                    let tree = Tree::new(initial_state, *is_view_focused);
                    let context = Context::new(
                        view.clone(),
                        tree,
                        Rc::clone(action_handler),
                        Rc::clone(&self.native_children),
                        *mtm,
                    );
                    let result = Rc::clone(&context);
                    self.state = State::Active(context);
                    result
                }
                None => {
                    let placeholder_update = TreeUpdate {
                        nodes: vec![(PLACEHOLDER_ROOT_ID, NodeProvider::new(Role::Window))],
                        tree: Some(TreeData::new(PLACEHOLDER_ROOT_ID)),
                        tree_id: TreeId::ROOT,
                        focus: PLACEHOLDER_ROOT_ID,
                    };
                    let placeholder_tree = Tree::new(placeholder_update, false);
                    let placeholder_context = Context::new(
                        view.clone(),
                        placeholder_tree,
                        Rc::new(ActionHandlerWrapper::new(PlaceholderActionHandler {})),
                        NativeChildren::default(),
                        *mtm,
                    );
                    let result = Rc::clone(&placeholder_context);
                    self.state = State::Placeholder {
                        placeholder_context,
                        is_view_focused: *is_view_focused,
                        action_handler: Rc::clone(action_handler),
                    };
                    result
                }
            },
            State::Placeholder {
                placeholder_context,
                ..
            } => Rc::clone(placeholder_context),
            State::Active(context) => Rc::clone(context),
        }
    }

    pub fn view_children<H: ActivationHandler + ?Sized>(
        &mut self,
        activation_handler: &mut H,
    ) -> *mut NSArray<NSObject> {
        let context = self.get_or_init_context(activation_handler);
        let tree = context.tree.borrow();
        let state = tree.state();
        let node = state.root();
        let platform_nodes = if filter(&node) == FilterResult::Include {
            vec![Id::into_super(Id::into_super(
                context.get_or_create_platform_node(node.id()),
            ))]
        } else {
            node.filtered_children(filter)
                .map(|node| {
                    Id::into_super(Id::into_super(
                        context.get_or_create_platform_node(node.id()),
                    ))
                })
                .collect::<Vec<Id<NSObject>>>()
        };
        let array = NSArray::from_vec(platform_nodes);
        Id::autorelease_return(array)
    }

    pub fn focus<H: ActivationHandler + ?Sized>(
        &mut self,
        activation_handler: &mut H,
    ) -> *mut NSObject {
        let context = self.get_or_init_context(activation_handler);
        let tree = context.tree.borrow();
        let state = tree.state();
        if let Some(node) = state.focus() {
            if can_be_focused(&node) {
                let native = context.native_children(&node).into_iter().find(|element| {
                    element.respondsToSelector(sel!(isAccessibilityFocused))
                        && unsafe { msg_send![&**element, isAccessibilityFocused] }
                });
                if let Some(element) = native {
                    return Id::autorelease_return(element);
                }
                return Id::autorelease_return(context.get_or_create_platform_node(node.id()))
                    as *mut _;
            }
        }
        null_mut()
    }

    fn weak_view(&self) -> &WeakId<NSView> {
        match &self.state {
            State::Inactive { view, .. } => view,
            State::Placeholder {
                placeholder_context,
                ..
            } => &placeholder_context.view,
            State::Active(context) => &context.view,
        }
    }

    pub fn hit_test<H: ActivationHandler + ?Sized>(
        &mut self,
        point: NSPoint,
        activation_handler: &mut H,
    ) -> *mut NSObject {
        let view = match self.weak_view().load() {
            Some(view) => view,
            None => {
                return null_mut();
            }
        };

        let context = self.get_or_init_context(activation_handler);
        let tree = context.tree.borrow();
        let state = tree.state();
        let root = state.root();
        let local_point = from_ns_point(&view, &root, point);
        let node = root.node_at_point(local_point, &filter).unwrap_or(root);
        // `point` is in screen coordinates, as are the frames of host-owned elements.
        for element in context.native_children(&node) {
            if !element.respondsToSelector(sel!(accessibilityFrame)) {
                continue;
            }
            let frame: NSRect = unsafe { msg_send![&*element, accessibilityFrame] };
            let contains = point.x >= frame.origin.x
                && point.x < frame.origin.x + frame.size.width
                && point.y >= frame.origin.y
                && point.y < frame.origin.y + frame.size.height;
            if !contains {
                continue;
            }
            let hit: Option<Id<NSObject>> =
                if element.respondsToSelector(sel!(accessibilityHitTest:)) {
                    unsafe { msg_send_id![&*element, accessibilityHitTest: point] }
                } else {
                    None
                };
            return Id::autorelease_return(hit.unwrap_or(element));
        }
        Id::autorelease_return(context.get_or_create_platform_node(node.id())) as *mut _
    }
}
