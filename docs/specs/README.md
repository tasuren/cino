# cino 仕様書セット（MVP）

## ドキュメント一覧

1. `core-language.md`
2. `static-semantics.md`
3. `runtime-memory-rust.md`
4. `ir-codegen-rust.md`
5. `host-abi-ffi.md`
6. `docgen-spec.md`

## 推奨読了順

1. コア言語仕様
2. 静的意味論仕様
3. 実行系・メモリ仕様
4. IR・コード生成仕様
5. ホスト ABI・FFI 仕様
6. 仕様書生成仕様

## 目的

本セットは次の最小契約を定義する。

- 決定的なドメイン挙動
- Rust 優先インタプリタ/VM 実装
- 安定したホスト連携と仕様書生成

## 実装クレート（MVP 推奨）

- `cino-syntax`: 構文木とパーサ
- `cino-sema`: 型/純粋性/網羅性検査
- `cino-ir`: 型付きIRと lowering
- `cino-vm`: バイトコード実行器
- `cino-runtime`: 実行APIと状態ハンドル管理
- `cino-ffi-c`: C ABI 公開層
- `cino-docgen`: 仕様書生成
- `cino-cli`: 開発者向けCLI
