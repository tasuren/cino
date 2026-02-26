//! counter_app — Integration sample of cino and libui
//!
//! Loads counter.cino at runtime,
//! and demonstrates delegating state management of a counter app to cino
//! by dispatching events and executing queries from libui GUI widgets.
//!
//! # How to run
//! ```
//! cd examples/counter_app
//! cargo run
//! ```

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use libui::controls::{Button, HorizontalBox, Label, LayoutStrategy, VerticalBox};
use libui::prelude::*;

use cino_ir::lower_program;
use cino_runtime::{Runtime, StateHandle};
use cino_syntax::parse_program;
use cino_vm::{IrVmProgram, VmValue};

/// Embed the source of counter.cino into the executable.
const COUNTER_CINO: &str = include_str!("../counter.cino");

// ──────────────────────────────────────────────
// cino load helpers
// ──────────────────────────────────────────────

/// Parses and compiles counter.cino and returns a Runtime.
fn load_runtime() -> Runtime {
    let program = parse_program(COUNTER_CINO).expect("Failed to parse counter.cino");

    let lowered = lower_program(&program);
    if !lowered.diagnostics.is_empty() {
        for d in &lowered.diagnostics {
            eprintln!("[{}] {}:{}  {}", d.code, d.line, d.column, d.message);
        }
        panic!("Failed to lower counter.cino to IR");
    }

    let ir = lowered.program.expect("IR was not generated");
    let vm = IrVmProgram::from_ir(ir).expect("Failed to generate VM program");
    Runtime::new(Arc::new(vm))
}

// ──────────────────────────────────────────────
// Domain operation helpers
// ──────────────────────────────────────────────

/// Constructs a unit variant event, calls update, and returns a new StateHandle.
fn dispatch(runtime: &Runtime, state: &StateHandle, event_tag: &str) -> StateHandle {
    let event = VmValue::Enum {
        tag: event_tag.to_string(),
        fields: BTreeMap::new(),
    };
    runtime
        .update(state, &event)
        .unwrap_or_else(|e| panic!("Failed to update '{event_tag}': {:?}", e))
        .state
}

/// Executes the GetCount query and returns the count.
/// Since the query in counter.cino returns Result<Int, Int>, extract the Ok { v } variant.
fn get_count(runtime: &Runtime, state: &StateHandle) -> i64 {
    let q = VmValue::Enum {
        tag: "GetCount".to_string(),
        fields: BTreeMap::new(),
    };
    let result = runtime
        .query(state, &q)
        .expect("Failed to execute query")
        .result;
    match &result {
        VmValue::Enum { tag, fields } if tag == "Ok" => {
            if let Some(VmValue::Int(n)) = fields.get("v") {
                *n
            } else {
                0
            }
        }
        _ => panic!("Unexpected query result: {:?}", result),
    }
}

// ──────────────────────────────────────────────
// Entry point
// ──────────────────────────────────────────────

fn main() {
    // Wrap the cino runtime in an Arc to share it across multiple callbacks.
    let runtime = Arc::new(load_runtime());

    // State is shared within the GUI thread via Rc<RefCell<StateHandle>>.
    let state = Rc::new(RefCell::new(StateHandle::from_value(VmValue::Int(0))));

    // ── libui initialization ──────────────────
    let ui = UI::init().expect("Failed to initialize libui");

    let mut win = Window::new(
        &ui.clone(),
        "Counter (cino)",
        300,
        120,
        WindowType::NoMenubar,
    );

    // ── Layout ────────────────────────────────
    let mut vbox = VerticalBox::new();
    vbox.set_padded(true);

    // Count display label
    let count_label = Label::new("Count: 0");

    // Button row
    let mut hbox = HorizontalBox::new();
    hbox.set_padded(true);

    let mut btn_inc = Button::new("+1");
    let mut btn_dec = Button::new("-1");
    let mut btn_reset = Button::new("Reset");

    // ── Callbacks ─────────────────────────────
    // Each button follows the same structure: dispatch event -> query count -> update label
    btn_inc.on_clicked({
        let runtime = Arc::clone(&runtime);
        let state = Rc::clone(&state);
        let mut label = count_label.clone();
        move |_btn| {
            let next = dispatch(&runtime, &state.borrow(), "Increment");
            *state.borrow_mut() = next;
            label.set_text(&format!("Count: {}", get_count(&runtime, &state.borrow())));
        }
    });

    btn_dec.on_clicked({
        let runtime = Arc::clone(&runtime);
        let state = Rc::clone(&state);
        let mut label = count_label.clone();
        move |_btn| {
            let next = dispatch(&runtime, &state.borrow(), "Decrement");
            *state.borrow_mut() = next;
            label.set_text(&format!("Count: {}", get_count(&runtime, &state.borrow())));
        }
    });

    btn_reset.on_clicked({
        let runtime = Arc::clone(&runtime);
        let state = Rc::clone(&state);
        let mut label = count_label.clone();
        move |_btn| {
            let next = dispatch(&runtime, &state.borrow(), "Reset");
            *state.borrow_mut() = next;
            label.set_text(&format!("Count: {}", get_count(&runtime, &state.borrow())));
        }
    });

    // ── Assemble widget tree ──────────────────
    hbox.append(btn_inc, LayoutStrategy::Stretchy);
    hbox.append(btn_dec, LayoutStrategy::Stretchy);
    hbox.append(btn_reset, LayoutStrategy::Stretchy);

    vbox.append(count_label, LayoutStrategy::Compact);
    vbox.append(hbox, LayoutStrategy::Compact);

    win.set_child(vbox);
    win.show();
    ui.main();
}
