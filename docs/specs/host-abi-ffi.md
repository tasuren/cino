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

MVP では次のどちらかを採用して固定する。

1. ABI ネイティブの値ビルダ/ゲッタ
2. 正準バイナリ（または JSON）エンコード/デコード

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
