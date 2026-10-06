// Copyright 2026 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

//! Host-owned native elements attached to AccessKit nodes.

use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, Node, NodeId, Rect, Role, Tree, TreeId,
    TreeUpdate,
};
use accesskit_macos::Adapter;
use objc2::{msg_send, msg_send_id, rc::Id, runtime::AnyObject};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityElement, NSApplication, NSBackingStoreType, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSArray, NSObject, NSPoint, NSRect, NSSize};
use std::ffi::c_void;

const ROOT: NodeId = NodeId(0);
const PANE: NodeId = NodeId(1);
const CAPTION: NodeId = NodeId(2);

struct InitialTree;

impl ActivationHandler for InitialTree {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let mut root = Node::new(Role::Window);
        root.set_bounds(Rect::new(0.0, 0.0, 400.0, 400.0));
        root.set_children(vec![PANE]);
        let mut pane = Node::new(Role::Group);
        pane.set_label("Pane");
        pane.set_bounds(Rect::new(0.0, 0.0, 400.0, 400.0));
        pane.set_children(vec![CAPTION]);
        let mut caption = Node::new(Role::Button);
        caption.set_label("Close Pane");
        caption.set_bounds(Rect::new(0.0, 0.0, 400.0, 20.0));
        Some(TreeUpdate {
            nodes: vec![(ROOT, root), (PANE, pane), (CAPTION, caption)],
            tree: Some(Tree::new(ROOT)),
            tree_id: TreeId::ROOT,
            focus: PANE,
        })
    }
}

struct NoActions;

impl ActionHandler for NoActions {
    fn do_action(&mut self, _: ActionRequest) {}
}

fn children(element: &AnyObject) -> Vec<Id<NSObject>> {
    let children: Option<Id<NSArray<NSObject>>> =
        unsafe { msg_send_id![element, accessibilityChildren] };
    children
        .map(|children| children.to_vec_retained())
        .unwrap_or_default()
}

fn object(pointer: *mut NSObject) -> Id<NSObject> {
    unsafe { Id::retain(pointer) }.expect("the adapter returns an element")
}

fn main() {
    let mtm = MainThreadMarker::new().expect("the test runs on the main thread");
    let _application = NSApplication::sharedApplication(mtm);
    let content = NSRect::new(NSPoint::new(100.0, 100.0), NSSize::new(200.0, 200.0));
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            content,
            NSWindowStyleMask::Titled,
            NSBackingStoreType::NSBackingStoreBuffered,
            false,
        )
    };
    unsafe { window.setReleasedWhenClosed(false) };
    let view = window.contentView().expect("the window has a content view");
    let mut adapter = unsafe { Adapter::new(Id::as_ptr(&view) as *mut c_void, true, NoActions) };
    let mut activation = InitialTree;

    let roots = adapter.view_children(&mut activation);
    let roots = unsafe { Id::retain(roots) }.unwrap().to_vec_retained();
    assert_eq!(roots.len(), 1, "the window node is the view's only child");
    let pane = children(&roots[0]).remove(0);

    let screen_frame = window.convertRectToScreen(view.frame());
    let text_area = unsafe { NSAccessibilityElement::new() };
    unsafe {
        text_area.setAccessibilityFrame(NSRect::new(
            screen_frame.origin,
            NSSize::new(screen_frame.size.width, screen_frame.size.height / 2.0),
        ));
        text_area.setAccessibilityFocused(true);
        adapter.set_native_children([(PANE, vec![Id::as_ptr(&text_area) as *mut c_void])]);
    }

    let pane_children = children(&pane);
    assert_eq!(
        pane_children.len(),
        2,
        "the native element follows the AccessKit child"
    );
    assert!(
        std::ptr::eq(Id::as_ptr(&pane_children[1]), Id::as_ptr(&text_area).cast()),
        "the native element is the pane's trailing child"
    );
    let parent: Option<Id<NSObject>> = unsafe { msg_send_id![&text_area, accessibilityParent] };
    assert!(
        std::ptr::eq(Id::as_ptr(&parent.unwrap()), Id::as_ptr(&pane)),
        "the native element's parent is its node"
    );

    let inside = NSPoint::new(screen_frame.origin.x + 10.0, screen_frame.origin.y + 10.0);
    let hit = object(adapter.hit_test(inside, &mut activation));
    assert!(
        std::ptr::eq(Id::as_ptr(&hit), Id::as_ptr(&text_area).cast()),
        "hit testing inside the native frame returns the native element"
    );
    let outside = NSPoint::new(
        screen_frame.origin.x + 10.0,
        screen_frame.origin.y + screen_frame.size.height - 10.0,
    );
    let hit = object(adapter.hit_test(outside, &mut activation));
    assert!(
        !std::ptr::eq(Id::as_ptr(&hit), Id::as_ptr(&text_area).cast()),
        "hit testing outside the native frame returns an AccessKit node"
    );

    let focus = object(adapter.focus(&mut activation));
    assert!(
        std::ptr::eq(Id::as_ptr(&focus), Id::as_ptr(&text_area).cast()),
        "a focused native element represents its focused node"
    );
    unsafe { text_area.setAccessibilityFocused(false) };
    let focus = object(adapter.focus(&mut activation));
    assert!(
        std::ptr::eq(Id::as_ptr(&focus), Id::as_ptr(&pane)),
        "an unfocused native element leaves focus on its node"
    );

    unsafe { adapter.set_native_children([]) };
    assert_eq!(
        children(&pane).len(),
        1,
        "replacing attachments removes the native element"
    );
    let _: () = unsafe { msg_send![&window, close] };
    println!("native_children: ok");
}
