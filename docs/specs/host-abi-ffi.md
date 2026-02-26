# cino ホスト ABI・FFI 仕様（ドラフト）

## 1. 対象範囲

本書はホスト連携契約を定義する。
低レベル正準インターフェースは C ABI とする。
Rust クレート API は C ABI の安全ラッパとして提供する。

## 2. 不透明ハンドル

ホスト可視の不透明型:

- `cino_program_t`
- `cino_state_t`
- `cino_value_t`（必要時）
- `cino_actions_t`
- `cino_error_t`

ホスト側は内部レイアウトへアクセスしない。

## 3. ライフサイクル API（概念）

- プログラム生成/読込
- 初期 state 生成
- update 実行
- query 実行
- result/action/error 解放

生成されたハンドルには必ず対になる `destroy/free` を用意する。

## 4. Update/Query 契約

- update 入力: `(state, event)`
- update 出力: `(new_state, actions)` または `error`
- query 入力: `(state, query)`
- query 出力: `result` または `error`

例外は ABI 境界を越えない。

## 5. 所有権規則

- 返却ハンドルの所有権は呼び出し側にある
- 引数渡しで所有権は移動しない（明示APIを除く）
- 所有権移動APIは命名と仕様で明示する

## 6. シリアライズ境界

MVP では CBOR（`cino-codec` クレート）に固定する。

- `cino_value_t` / `cino_actions_t` は内部で CBOR バイト列を保持する
- ホストは `cino_value_new_from_cbor` で CBOR バイト列を渡し、`cino_value_bytes` / `cino_actions_bytes` で取り出す
- CBOR エンコードは RFC 8949 Core Deterministic Encoding Requirements に準拠する
- JSON はデバッグ用途にのみ使用し、ABI の正準フォーマットとして扱わない

## 7. エラーモデル

すべての失敗は明示値で返す。

- コンパイル/読込失敗
- 検証/型エラー
- 実行時上限超過
- 不正ハンドル/API誤用

各エラーは次を持つ。

- 安定したエラーコード
- 人間可読メッセージ
- 任意のソース位置

## 8. スレッド規則

- ハンドルごとのスレッド安全性を明示する
- 非スレッド安全の場合、外部同期を要求する

## 9. WASM 注記

WASM API も C ABI と同じ意味契約を守る。

- 不透明 state
- 明示的 update/query
- 明示的エラー値

## 10. 実装クレート配置

C ABI の実装は `cino-ffi-c` クレートに集約する。
`cino-ffi-c` は `cino-runtime` を呼び出す薄い境界層として実装し、ドメイン評価ロジックを持たない。

この方針により、ホスト公開契約と内部実行系の責務を分離する。

## 11. MVP C ABI（確定）

MVP では、シリアライズ境界を CBOR に固定する。
`cino_value_t` / `cino_actions_t` は内部で CBOR バイト列を保持し、必要時に VM 値へデコードして利用する。

### 11.1 返却規約

すべての関数は `cino_status_t` を返す。

- `CINO_STATUS_OK`: 成功
- `CINO_STATUS_ERR`: 失敗（`out_error` が設定される）

`out_*` ポインタは成功時のみ書き込み、失敗時は不変とする。

### 11.2 不透明ハンドル

- `cino_program_t`
- `cino_state_t`
- `cino_value_t`
- `cino_actions_t`
- `cino_error_t`

### 11.3 主要 API

- `cino_program_new_mock_counter(out_program, out_error)`
- `cino_program_destroy(program)`
- `cino_state_new(program, initial_value, out_state, out_error)`
- `cino_state_destroy(state)`
- `cino_state_to_value(state, out_value, out_error)`
- `cino_update(program, state, event, out_next_state, out_actions, out_error)`
- `cino_query(program, state, query, out_result, out_error)`
- `cino_value_new_from_cbor(data, len, out_value, out_error)`
- `cino_value_destroy(value)`
- `cino_value_bytes(value, out_ptr, out_len)`
- `cino_actions_destroy(actions)`
- `cino_actions_bytes(actions, out_ptr, out_len)`
- `cino_error_destroy(error)`
- `cino_error_code(error)`
- `cino_error_message(error)`

### 11.4 所有権/解放

- `new` / `update` / `query` / `to_value` が返すハンドル所有権は呼び出し側にある
- 呼び出し側は対応する `destroy`/`free` を必ず呼ぶ
- `bytes` API が返すポインタはハンドル所有メモリを指し、ハンドル破棄まで有効

### 11.5 エラーコード

- `CINO_ERROR_RUNTIME_STEP_LIMIT_EXCEEDED`
- `CINO_ERROR_RUNTIME_MEMORY_LIMIT_EXCEEDED`
- `CINO_ERROR_RUNTIME_RECURSION_LIMIT_EXCEEDED`
- `CINO_ERROR_RUNTIME_INVALID_INPUT`
- `CINO_ERROR_RUNTIME_TRAP`
- `CINO_ERROR_RUNTIME_PANIC`
- `CINO_ERROR_ABI_NULL_POINTER`
- `CINO_ERROR_ABI_INVALID_CBOR`
- `CINO_ERROR_ABI_INVALID_HANDLE`
- `CINO_ERROR_ABI_INTERNAL`

### 11.6 互換性注意

MVP `program` 生成は `mock_counter` のみ提供する。
将来、IR/バイトコード読込 API を追加する際は後方互換を維持し、既存関数の意味を変更しない。
