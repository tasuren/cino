# cino 静的意味論仕様（MVP）

## 1. 対象範囲

本書は cino MVP の静的検査契約を定義する。

- 型検査
- 純粋性検査
- `fn` 制約検査（決定性・再帰上限）
- `match` 網羅性/到達不能検査
- 診断コード体系

本書で規定した静的規則違反はコンパイルエラーとして報告し、`file:line:column` を必須で付与する。
実行時制約（再帰深さ上限など）は実行時エラーとして報告する。

## 2. 基本方針

- レコード型は構造的同値で判定する
- 列挙型（`event` / `query` / `enum`）は名前的同値で判定する
- 暗黙型変換は行わない（明示的な構文が仕様化されるまで不許可）
- 同一入力は同一出力を返す（決定性）

## 3. 型規則

### 3.1 組み込み/ジェネリクス型

- リテラル型: `Int`, `Decimal`, `Bool`, `String`
- ジェネリクス: `List<T>`, `Map<K, V>`, `Option<T>`, `Result<T, E>`
- ユーザー型: `record`, `enum`, `state`, `event`, `query`

`Map<K, V>` の `K` は決定的な比較が可能な型でなければならない。

### 3.2 関数シグネチャ

- `update(state: S, event: E) -> (S, List<Action>)`
- `query(state: S, q: Q) -> Result<R, Err>`
- `fn name(...) -> T`

`update` / `query` の引数個数・戻り値形は固定契約であり、適合しない宣言は不正とする。

成功例:

```cino
update(state: BillingState, event: BillingEvent) -> (BillingState, List<Action>) {
  (state, [])
}
```

失敗例（戻り値契約違反）:

```cino
update(state: BillingState, event: BillingEvent) -> BillingState {
  state
}
```

### 3.3 ブロック式戻り値規則

- `fn` / `update` / `query` の本体は式ブロックとして扱う
- ブロックの最後の式が戻り値となる
- `return` 文は MVP では不許可

成功例:

```cino
fn score(base: Int, bonus: Int) -> Int {
  let doubled = base * 2
  doubled + bonus
}
```

失敗例（`return` 非対応）:

```cino
fn score(base: Int, bonus: Int) -> Int {
  return base + bonus
}
```

## 4. 純粋性規則

### 4.1 禁止操作

以下は `fn` / `update` / `query` で禁止する。

- I/O
- 時刻取得
- 乱数生成
- 可変グローバル状態
- 例外送出/捕捉
- 外部ライブラリ・外部関数の直接呼び出し

### 4.2 許可操作

- 不変なローカル束縛
- 純粋式評価
- 純粋な `fn` 呼び出し
- `Action` 値の構築（実行は行わない）

成功例:

```cino
fn can_issue(balance: Decimal, limit: Decimal) -> Bool {
  balance <= limit
}
```

失敗例（不純操作）:

```cino
fn should_retry() -> Bool {
  now() > 0
}
```

## 5. `fn` 規則（MVP）

- `fn` はトップレベル宣言のみ許可（入れ子関数は禁止）
- `fn` は純粋かつ決定的でなければならない
- `update` / `query` から呼び出せるのは検証済み `fn` のみ
- 再帰呼び出し（直接・間接）は許可する
- 再帰深さはランタイム上限 `max_recursion_depth` を超えてはならない

注記:

- 上限値自体は実行設定（`docs/specs/runtime-memory-rust.md`）で定義する
- 静的意味論は「再帰が許可されること」と「上限超過が実行時エラーであること」を契約化する

成功例（再帰）:

```cino
fn fact(n: Int) -> Int {
  match n {
    0 => 1
    _ => n * fact(n - 1)
  }
}
```

失敗例（入れ子 `fn`）:

```cino
fn outer(x: Int) -> Int {
  fn inner(y: Int) -> Int { y + 1 }
  inner(x)
}
```

## 6. `match` 規則

- `event` / `query` / `enum` に対する `match` は網羅必須
- ワイルドカード `_` は残り全ケースを網羅する
- 既に網羅済みの後続アームは到達不能エラーとする
- ガード付きパターンは MVP 非対応

成功例（網羅）:

```cino
match event {
  InvoiceIssued { id, amount } => ...
  PaymentReceived { id, amount } => ...
}
```

失敗例（非網羅）:

```cino
match event {
  InvoiceIssued { id, amount } => ...
}
```

失敗例（到達不能）:

```cino
match status {
  _ => 0
  Closed => 1
}
```

## 7. MVP 非対応事項

以下は MVP では仕様外（使用時はコンパイルエラー）とする。

- クロージャ式
- `return` 文
- `match` ガード（`if` 付きアーム）

## 8. 診断コード体系

### 8.1 `E-TYPE-*`（型）

- `E-TYPE-001`: 型不一致
- `E-TYPE-002`: 未解決シンボル
- `E-TYPE-003`: ジェネリクス引数数不一致
- `E-TYPE-004`: 不正な `update/query` シグネチャ
- `E-TYPE-005`: `Map<K, V>` の `K` が比較不能

### 8.2 `E-PURE-*`（純粋性）

- `E-PURE-001`: 禁止副作用操作の使用
- `E-PURE-002`: 外部関数/外部ライブラリ呼び出し

### 8.3 `E-FN-*`（関数規則）

- `E-FN-001`: `fn` がトップレベル以外で宣言されている
- `E-FN-002`: `fn` 本体が純粋性規則に違反
- `E-FN-003`: `return` 文の使用（MVP 非対応）
- `E-FN-004`: 再帰深さ上限超過（実行時エラーコード）

### 8.4 `E-MATCH-*`（パターンマッチ）

- `E-MATCH-001`: `match` が非網羅
- `E-MATCH-002`: 到達不能アーム
- `E-MATCH-003`: ガード付きアームの使用（MVP 非対応）

### 8.5 `E-UNSUPPORTED-*`（MVP 非対応）

- `E-UNSUPPORTED-001`: クロージャ式の使用
