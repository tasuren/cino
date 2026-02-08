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

## 5. データ型

組み込み型:

- `Int`, `Decimal`, `Bool`, `String`
- `List<T>`, `Map<K, V>`
- `Option<T>`, `Result<T, E>`
- ユーザー定義 `enum` / `record`

## 6. パターンマッチ

- `event` / `query` / 任意の `enum` に対する `match` は網羅必須
- 非網羅はコンパイルエラー
- フォールスルーは認めない

## 7. 評価契約

- `update` は純粋かつ決定的
- `query` は純粋かつ決定的
- 同一入力は同一出力を返す
- 外界変化は `Event` 注入でのみ表現する

## 8. エラーモデル

- 例外は使わない
- ドメイン失敗は `Result` で明示する

## 9. 安定性ルール

将来の言語拡張は、決定性と副作用禁止を壊してはならない。
