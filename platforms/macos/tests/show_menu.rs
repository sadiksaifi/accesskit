// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

//! Context menu actions exposed through the macOS accessibility interface.

use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Role, Tree, TreeId,
    TreeUpdate,
};
use accesskit_macos::Adapter;
use objc2::{msg_send, msg_send_id, rc::Id, sel};
use objc2_app_kit::{NSApplication, NSView};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSRect};
use std::{cell::RefCell, ffi::c_void, rc::Rc};

const ROOT: NodeId = NodeId(0);
const TAB: NodeId = NodeId(1);
const ROW: NodeId = NodeId(2);
const WITHOUT_MENU: NodeId = NodeId(3);
const GROUP: NodeId = NodeId(4);
const FILTERED_CONTAINER: NodeId = NodeId(5);
const INHERITED_MENU: NodeId = NodeId(6);

struct InitialTree;

impl ActivationHandler for InitialTree {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![TAB, ROW, WITHOUT_MENU, GROUP]);
        let mut tab = Node::new(Role::Tab);
        tab.add_action(Action::Click);
        tab.add_action(Action::ShowContextMenu);
        let mut row = Node::new(Role::Row);
        row.set_selected(false);
        row.add_action(Action::Click);
        row.add_action(Action::ShowContextMenu);
        let mut without_menu = Node::new(Role::Tab);
        without_menu.add_action(Action::Click);
        let mut group = Node::new(Role::Group);
        group.add_child_action(Action::ShowContextMenu);
        group.set_children(vec![FILTERED_CONTAINER]);
        let mut container = Node::new(Role::GenericContainer);
        container.set_children(vec![INHERITED_MENU]);
        Some(TreeUpdate {
            nodes: vec![
                (ROOT, root),
                (TAB, tab),
                (ROW, row),
                (WITHOUT_MENU, without_menu),
                (GROUP, group),
                (FILTERED_CONTAINER, container),
                (INHERITED_MENU, Node::new(Role::Tab)),
            ],
            tree: Some(Tree::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus: ROOT,
        })
    }
}

struct Actions(Rc<RefCell<Vec<ActionRequest>>>);

impl ActionHandler for Actions {
    fn do_action(&mut self, request: ActionRequest) {
        self.0.borrow_mut().push(request);
    }
}

fn children(element: &NSObject) -> Vec<Id<NSObject>> {
    let children: Id<NSArray<NSObject>> = unsafe { msg_send_id![element, accessibilityChildren] };
    children.to_vec_retained()
}

fn main() {
    let mtm = MainThreadMarker::new().expect("the test runs on the main thread");
    let _application = NSApplication::sharedApplication(mtm);
    let view = unsafe { NSView::initWithFrame(mtm.alloc(), NSRect::default()) };
    let requests = Rc::new(RefCell::new(Vec::new()));
    let mut adapter = unsafe {
        Adapter::new(
            Id::as_ptr(&view) as *mut c_void,
            true,
            Actions(Rc::clone(&requests)),
        )
    };
    let roots = adapter.view_children(&mut InitialTree);
    let roots = unsafe { Id::retain(roots) }
        .expect("the adapter returns its root elements")
        .to_vec_retained();
    assert_eq!(roots.len(), 1);
    let root_children = children(&roots[0]);
    assert_eq!(root_children.len(), 4);

    // AppKit discovers AXShowMenu through the allowed modern selector,
    // just as it discovers AXPress and AXPick.
    for (index, target_node) in [(0, TAB), (1, ROW)] {
        let allowed: bool = unsafe {
            msg_send![&root_children[index], isAccessibilitySelectorAllowed: sel!(accessibilityPerformShowMenu)]
        };
        assert!(allowed, "Show Menu is allowed for {target_node:?}");
        let performed: bool =
            unsafe { msg_send![&root_children[index], accessibilityPerformShowMenu] };
        assert!(performed, "Show Menu succeeds for {target_node:?}");
    }

    let allowed: bool = unsafe {
        msg_send![&root_children[2], isAccessibilitySelectorAllowed: sel!(accessibilityPerformShowMenu)]
    };
    assert!(!allowed, "Show Menu is unavailable without the action");
    let performed: bool = unsafe { msg_send![&root_children[2], accessibilityPerformShowMenu] };
    assert!(!performed, "an unsupported action does not succeed");

    let inherited = children(&root_children[3]);
    assert_eq!(inherited.len(), 1, "the generic container is filtered out");
    let allowed: bool = unsafe {
        msg_send![&inherited[0], isAccessibilitySelectorAllowed: sel!(accessibilityPerformShowMenu)]
    };
    assert!(allowed, "Show Menu is inherited from the filtered parent");
    let performed: bool = unsafe { msg_send![&inherited[0], accessibilityPerformShowMenu] };
    assert!(performed, "the inherited action succeeds");

    assert_eq!(
        *requests.borrow(),
        [TAB, ROW, INHERITED_MENU]
            .map(|target_node| ActionRequest {
                action: Action::ShowContextMenu,
                target_tree: TreeId::ROOT,
                target_node,
                data: None,
            })
            .to_vec(),
        "only supported nodes send ShowContextMenu requests"
    );
    println!("show_menu: ok");
}
