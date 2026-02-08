# cino コア言語仕様（ドラフト）

## 1. 対象範囲

本書は cino の中核言語モデルを定義する。
cino は直接的な副作用を持たない、決定的なドメイン定義言語である。

## 2. 中核モデル

- `State`: 不透明なドメイン状態
- `Event`: 状態遷移を引き起こす入力
- `Update`: `State x Event -> (State, List<Action>)`
- `Query`: 状態を読み取る要求
- `Action`: ホスト実行系に渡す副作用要求

## 3. 純粋性と禁止操作

ユーザープログラムでは以下を禁止する。

- I/O
- 時刻取得
- 乱数生成
- グローバル可変状態
- 例外の送出/捕捉
- 外部ライブラリの直接呼び出し

副作用境界は `Action` の生成のみとする。

## 4. 宣言

トップレベル宣言は次に限定する。

- `state`（不変レコード）
- `event`（タグ付きユニオン）
- `query`（タグ付きユニオン）
- `result` / ドメイン `enum` / `record`
- `update` 関数
- `query` 関数

例:

```cino
state BillingState {
  invoices: Map<InvoiceId, Invoice>
  balance: Decimal
}

event BillingEvent =
  | InvoiceIssued { id: InvoiceId, amount: Decimal }
  | PaymentReceived { id: InvoiceId, amount: Decimal }

query BillingQuery =
  | CurrentBalance
  | InvoiceStatus { id: InvoiceId }

update(state: BillingState, event: BillingEvent) -> (BillingState, List<Action>) = ...
query(state: BillingState, q: BillingQuery) -> Result<QueryResult, DomainError> = ...
```

## 5. 最小構文 EBNF（MVP）

### 5.1 記法

- `"`で囲むものは予約語または記号の終端記号
- `A | B` は選択
- `{ X }` は 0 回以上繰り返し
- `[ X ]` は 0 回または 1 回

### 5.2 文法

```ebnf
program         = { top_decl } ;
top_decl        = state_decl
                | event_decl
                | query_decl
                | enum_decl
                | record_decl
                | user_fn_decl
                | update_fn_decl
                | query_fn_decl ;

state_decl      = "state" type_name record_body ;
event_decl      = "event" type_name "=" variant_list ;
query_decl      = "query" type_name "=" variant_list ;
enum_decl       = "enum" type_name "=" variant_list ;
record_decl     = "record" type_name record_body ;

update_fn_decl  = "update" "(" param "," param ")" "->" tuple_type block ;
query_fn_decl   = "query" "(" param "," param ")" "->" type_expr block ;
user_fn_decl    = "fn" ident "(" [ param { "," param } ] ")" "->" type_expr block ;

variant_list    = variant { variant } ;
variant         = "|" ctor_name [ record_payload ] ;
record_body     = "{" { field_decl } "}" ;
record_payload  = "{" { field_decl } "}" ;
field_decl      = ident ":" type_expr ;
param           = ident ":" type_expr ;

tuple_type      = "(" type_expr "," type_expr ")" ;
type_expr       = simple_type | generic_type ;
simple_type     = type_name ;
generic_type    = type_name "<" type_expr { "," type_expr } ">" ;

match_expr      = "match" expr "{" { match_arm } "}" ;
match_arm       = pattern "=>" expr ;
pattern         = ctor_name [ "{" { pat_field } "}" ] | "_" ;
pat_field       = ident [ ":" pattern ] ;

result_type     = "Result" "<" type_expr "," type_expr ">" ;
option_type     = "Option" "<" type_expr ">" ;

type_name       = ident ;
ctor_name       = ident ;
ident           = IDENT ;
expr            = EXPR ;
block           = "{" { stmt } expr "}" ;
stmt            = let_stmt ;
let_stmt        = "let" ident "=" expr ;
```

### 5.3 未確定事項（TODO）

- TODO: `IDENT` と予約語衝突回避を含む字句規則（Unicode 許可範囲を含む）
- TODO: `EXPR` の優先順位/結合規則と最小演算子集合
- TODO: `variant_list` の行区切り（改行必須か、`;` 区切りを許可するか）
- TODO: `query` キーワード（型宣言と関数宣言）を将来分離するかどうか
- TODO: `Map<K, V>` の `K` 制約（比較可能性）を構文か静的意味論のどちらで表現するか

## 6. 構文サンプル（MVP）

### 6.1 `state`

```cino
state BillingState {
  balance: Decimal
  last_invoice_id: Option<InvoiceId>
}
```

### 6.2 `event`

```cino
event BillingEvent =
  | InvoiceIssued { id: InvoiceId, amount: Decimal }
  | PaymentReceived { id: InvoiceId, amount: Decimal }
```

### 6.3 `query`（宣言）

```cino
query BillingQuery =
  | CurrentBalance
  | InvoiceStatus { id: InvoiceId }
```

### 6.4 `update` 関数宣言

```cino
update(state: BillingState, event: BillingEvent) -> (BillingState, List<Action>) {
  ...
}
```

### 6.5 `query` 関数宣言

```cino
query(state: BillingState, q: BillingQuery) -> Result<QueryResult, DomainError> {
  ...
}
```

### 6.6 `enum`

```cino
enum InvoiceStatus =
  | Draft
  | Open
  | Closed
```

### 6.7 `record`

```cino
record Invoice {
  id: InvoiceId
  amount: Decimal
}
```

### 6.8 `match`

```cino
match event {
  InvoiceIssued { id, amount } => ...
  PaymentReceived { id, amount } => ...
}
```

### 6.9 `Result`

```cino
query(state: BillingState, q: BillingQuery) -> Result<QueryResult, DomainError> {
  ...
}
```

### 6.10 `Option`

```cino
record BillingState {
  last_invoice_id: Option<InvoiceId>
}
```

### 6.11 ユーザー定義関数 `fn`

```cino
fn can_issue(balance: Decimal, limit: Decimal) -> Bool {
  balance <= limit
}
```

### 6.12 複数行の式ブロック

```cino
fn score(base: Int, bonus: Int) -> Int {
  let doubled = base * 2
  let total = doubled + bonus
  total
}
```

## 7. データ型

組み込み型:

- `Int`, `Decimal`, `Bool`, `String`
- `List<T>`, `Map<K, V>`
- `Option<T>`, `Result<T, E>`
- ユーザー定義 `enum` / `record`

## 8. パターンマッチ

- `event` / `query` / 任意の `enum` に対する `match` は網羅必須
- 非網羅はコンパイルエラー
- フォールスルーは認めない

## 9. 評価契約

- `fn`（ユーザー定義関数）は純粋かつ決定的でなければならない
- `update` は純粋かつ決定的
- `query` は純粋かつ決定的
- 同一入力は同一出力を返す
- 外界変化は `Event` 注入でのみ表現する

## 10. エラーモデル

- 例外は使わない
- ドメイン失敗は `Result` で明示する

## 11. 安定性ルール

将来の言語拡張は、決定性と副作用禁止を壊してはならない。
