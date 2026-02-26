//! counter_app — cino × libui のインテグレーションサンプル
//!
//! counter.cino をランタイムにロードし、
//! libui の GUI ウィジェットからイベントを投入・クエリを実行することで
//! カウンターアプリの状態管理を cino に委ねるデモです。
//!
//! # 実行方法
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

/// counter.cino のソースを実行バイナリに埋め込む。
const COUNTER_CINO: &str = include_str!("../counter.cino");

// ──────────────────────────────────────────────
// cino ロードヘルパー
// ──────────────────────────────────────────────

/// counter.cino をパース・コンパイルして Runtime を返す。
fn load_runtime() -> Runtime {
    let program = parse_program(COUNTER_CINO).expect("counter.cino のパースに失敗しました");

    let lowered = lower_program(&program);
    if !lowered.diagnostics.is_empty() {
        for d in &lowered.diagnostics {
            eprintln!("[{}] {}:{}  {}", d.code, d.line, d.column, d.message);
        }
        panic!("counter.cino の IR 変換に失敗しました");
    }

    let ir = lowered.program.expect("IR が生成されませんでした");
    let vm = IrVmProgram::from_ir(ir).expect("VM プログラムの生成に失敗しました");
    Runtime::new(Arc::new(vm))
}

// ──────────────────────────────────────────────
// ドメイン操作ヘルパー
// ──────────────────────────────────────────────

/// unit variant のイベントを構築して update を呼び出し、新しい StateHandle を返す。
fn dispatch(runtime: &Runtime, state: &StateHandle, event_tag: &str) -> StateHandle {
    let event = VmValue::Enum {
        tag: event_tag.to_string(),
        fields: BTreeMap::new(),
    };
    runtime
        .update(state, &event)
        .unwrap_or_else(|e| panic!("update '{event_tag}' に失敗: {:?}", e))
        .state
}

/// GetCount クエリを実行してカウント値を返す。
/// counter.cino の query は Result<Int, Int> を返すので Ok { v } を取り出す。
fn get_count(runtime: &Runtime, state: &StateHandle) -> i64 {
    let q = VmValue::Enum {
        tag: "GetCount".to_string(),
        fields: BTreeMap::new(),
    };
    let result = runtime
        .query(state, &q)
        .expect("query に失敗しました")
        .result;
    match &result {
        VmValue::Enum { tag, fields } if tag == "Ok" => {
            if let Some(VmValue::Int(n)) = fields.get("v") {
                *n
            } else {
                0
            }
        }
        _ => panic!("予期しないクエリ結果: {:?}", result),
    }
}

// ──────────────────────────────────────────────
// エントリーポイント
// ──────────────────────────────────────────────

fn main() {
    // cino ランタイムを Arc で包んで複数のコールバックから共有する。
    let runtime = Arc::new(load_runtime());

    // 状態は Rc<RefCell<StateHandle>> で GUI スレッド内で共有する。
    let state = Rc::new(RefCell::new(StateHandle::from_value(VmValue::Int(0))));

    // ── libui 初期化 ──────────────────────────
    let ui = UI::init().expect("libui の初期化に失敗しました");

    let mut win = Window::new(
        &ui.clone(),
        "Counter (cino)",
        300,
        120,
        WindowType::NoMenubar,
    );

    // ── レイアウト ────────────────────────────
    let mut vbox = VerticalBox::new();
    vbox.set_padded(true);

    // カウント表示ラベル
    let count_label = Label::new("Count: 0");

    // ボタン行
    let mut hbox = HorizontalBox::new();
    hbox.set_padded(true);

    let mut btn_inc = Button::new("+1");
    let mut btn_dec = Button::new("-1");
    let mut btn_reset = Button::new("Reset");

    // ── コールバック ──────────────────────────
    // 各ボタンは同じ構造: イベントを dispatch → count を query → ラベル更新
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

    // ── ウィジェットツリー組み立て ────────────
    hbox.append(btn_inc, LayoutStrategy::Stretchy);
    hbox.append(btn_dec, LayoutStrategy::Stretchy);
    hbox.append(btn_reset, LayoutStrategy::Stretchy);

    vbox.append(count_label, LayoutStrategy::Compact);
    vbox.append(hbox, LayoutStrategy::Compact);

    win.set_child(vbox);
    win.show();
    ui.main();
}
