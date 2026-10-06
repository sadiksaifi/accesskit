// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

//! Heading roles exposed through the macOS accessibility interface.

use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Role, Tree, TreeId, TreeUpdate,
};
use accesskit_macos::Adapter;
use objc2::{msg_send_id, rc::Id};
use objc2_app_kit::{NSApplication, NSView};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSRect, NSString};
use std::ffi::c_void;

const ROOT: NodeId = NodeId(0);
const HEADING: NodeId = NodeId(1);
const SUBTITLE: NodeId = NodeId(2);

struct InitialTree;

impl ActivationHandler for InitialTree {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![HEADING, SUBTITLE]);
        Some(TreeUpdate {
            nodes: vec![
                (ROOT, root),
                (HEADING, Node::new(Role::Heading)),
                (SUBTITLE, Node::new(Role::DocSubtitle)),
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
    let children: Id<NSArray<NSObject>> = unsafe { msg_send_id![&roots[0], accessibilityChildren] };
    let children = children.to_vec_retained();
    assert_eq!(children.len(), 2);

    for (index, role) in [Role::Heading, Role::DocSubtitle].into_iter().enumerate() {
        let native_role: Id<NSString> =
            unsafe { msg_send_id![&children[index], accessibilityRole] };
        assert_eq!(native_role.to_string(), "AXHeading", "role: {role:?}");
    }
    println!("heading_roles: ok");
}
