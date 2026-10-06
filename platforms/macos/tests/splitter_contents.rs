// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

//! Splitter contents exposed through the macOS accessibility interface.

use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Orientation, Rect, Role, Tree,
    TreeId, TreeUpdate,
};
use accesskit_macos::Adapter;
use objc2::{msg_send, msg_send_id, rc::Id, sel};
use objc2_app_kit::{NSApplication, NSView};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSRect};
use std::ffi::c_void;

const ROOT: NodeId = NodeId(0);
const TOOLBAR: NodeId = NodeId(1);
const SIDEBAR: NodeId = NodeId(2);
const SPLITTER: NodeId = NodeId(3);
const CONTAINER: NodeId = NodeId(4);
const CONTENT: NodeId = NodeId(5);
const BOTTOM_SPLITTER: NodeId = NodeId(6);
const STATUS: NodeId = NodeId(7);
const UNORIENTED: NodeId = NodeId(8);
const SHOW_BUTTON: NodeId = NodeId(9);
const COLLAPSED_SPLITTER: NodeId = NodeId(10);
const DETAIL: NodeId = NodeId(11);

fn node(role: Role, bounds: (f64, f64, f64, f64)) -> Node {
    let mut node = Node::new(role);
    node.set_bounds(Rect::new(bounds.0, bounds.1, bounds.2, bounds.3));
    node
}

fn splitter(bounds: (f64, f64, f64, f64), orientation: Option<Orientation>) -> Node {
    let mut node = node(Role::Splitter, bounds);
    if let Some(orientation) = orientation {
        node.set_orientation(orientation);
    }
    node
}

struct InitialTree;

impl ActivationHandler for InitialTree {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![
            TOOLBAR,
            SIDEBAR,
            SPLITTER,
            CONTAINER,
            BOTTOM_SPLITTER,
            STATUS,
            UNORIENTED,
            SHOW_BUTTON,
            COLLAPSED_SPLITTER,
            DETAIL,
        ]);
        // At position zero, nothing lies before this splitter, though a button sits beside it.
        let mut collapsed = splitter((40.0, 610.0, 48.0, 700.0), Some(Orientation::Vertical));
        collapsed.set_numeric_value(0.0);
        let mut container = Node::new(Role::GenericContainer);
        container.set_children(vec![CONTENT]);
        Some(TreeUpdate {
            nodes: vec![
                (ROOT, root),
                // The toolbar spans both sides of the vertical splitter, so it lies on neither.
                (TOOLBAR, node(Role::Toolbar, (0.0, 0.0, 900.0, 30.0))),
                (SIDEBAR, node(Role::List, (0.0, 30.0, 240.0, 600.0))),
                (
                    SPLITTER,
                    splitter((236.0, 30.0, 244.0, 600.0), Some(Orientation::Vertical)),
                ),
                (CONTAINER, container),
                (CONTENT, node(Role::Group, (244.0, 30.0, 900.0, 500.0))),
                (
                    BOTTOM_SPLITTER,
                    splitter((244.0, 500.0, 900.0, 506.0), Some(Orientation::Horizontal)),
                ),
                (STATUS, node(Role::Group, (244.0, 506.0, 900.0, 600.0))),
                (UNORIENTED, splitter((0.0, 600.0, 900.0, 604.0), None)),
                (SHOW_BUTTON, node(Role::Button, (0.0, 620.0, 30.0, 650.0))),
                (COLLAPSED_SPLITTER, collapsed),
                (DETAIL, node(Role::Group, (48.0, 610.0, 900.0, 700.0))),
            ],
            tree: Some(Tree::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus: ROOT,
        })
    }
}

struct NoActions;

impl ActionHandler for NoActions {
    fn do_action(&mut self, _: ActionRequest) {}
}

fn children(element: &NSObject) -> Vec<Id<NSObject>> {
    let children: Id<NSArray<NSObject>> = unsafe { msg_send_id![element, accessibilityChildren] };
    children.to_vec_retained()
}

fn previous_contents(element: &NSObject) -> Vec<*const NSObject> {
    let contents: Id<NSArray<NSObject>> =
        unsafe { msg_send_id![element, accessibilityPreviousContents] };
    contents.to_vec_retained().iter().map(Id::as_ptr).collect()
}

fn next_contents(element: &NSObject) -> Vec<*const NSObject> {
    let contents: Id<NSArray<NSObject>> =
        unsafe { msg_send_id![element, accessibilityNextContents] };
    contents.to_vec_retained().iter().map(Id::as_ptr).collect()
}

fn allows_contents(element: &NSObject) -> bool {
    let previous: bool = unsafe {
        msg_send![element, isAccessibilitySelectorAllowed: sel!(accessibilityPreviousContents)]
    };
    let next: bool = unsafe {
        msg_send![element, isAccessibilitySelectorAllowed: sel!(accessibilityNextContents)]
    };
    assert_eq!(previous, next, "both sides are published together");
    previous
}

fn main() {
    let mtm = MainThreadMarker::new().expect("the test runs on the main thread");
    let _application = NSApplication::sharedApplication(mtm);
    let view = unsafe { NSView::initWithFrame(mtm.alloc(), NSRect::default()) };
    let mut adapter = unsafe { Adapter::new(Id::as_ptr(&view) as *mut c_void, true, NoActions) };
    let roots = adapter.view_children(&mut InitialTree);
    let roots = unsafe { Id::retain(roots) }
        .expect("the adapter returns its root elements")
        .to_vec_retained();
    assert_eq!(roots.len(), 1);
    let elements = children(&roots[0]);
    // The generic container is filtered out, so its content is the splitter's sibling.
    let [
        toolbar,
        sidebar,
        splitter,
        content,
        bottom_splitter,
        status,
        unoriented,
        show_button,
        collapsed_splitter,
        detail,
    ] = elements.as_slice()
    else {
        panic!("the window presents ten children, found {}", elements.len());
    };

    assert!(allows_contents(splitter));
    assert_eq!(previous_contents(splitter), [Id::as_ptr(sidebar)]);
    assert_eq!(
        next_contents(splitter),
        [Id::as_ptr(content), Id::as_ptr(status)],
        "other splitters lie on neither side"
    );

    assert!(allows_contents(bottom_splitter));
    assert_eq!(
        previous_contents(bottom_splitter),
        [Id::as_ptr(toolbar), Id::as_ptr(content)]
    );
    assert_eq!(
        next_contents(bottom_splitter),
        [Id::as_ptr(status), Id::as_ptr(detail)]
    );

    assert!(allows_contents(collapsed_splitter));
    assert_eq!(
        previous_contents(collapsed_splitter),
        [],
        "a splitter at position zero has collapsed the contents before it"
    );
    assert_eq!(next_contents(collapsed_splitter), [Id::as_ptr(detail)]);
    assert!(!allows_contents(show_button));

    assert!(
        !allows_contents(unoriented),
        "a splitter without an orientation has no sides"
    );
    assert!(!allows_contents(sidebar), "only splitters have contents");
    println!("splitter_contents: ok");
}
