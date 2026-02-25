# cino IR・コード生成仕様（Rust 優先, ドラフト）

## 1. 対象範囲

本書はコンパイル処理系を定義する。

- パーサAST
- 検証済みIR
- Rust VM 向けバイトコード（主経路）
- 任意の Rust ソース生成（副経路）

## 2. パイプライン

1. ソースを AST に構文解析
2. 名前解決とシンボル表構築
3. 型/純粋性/網羅性/クロージャ制約の検査
4. 型付き IR へ lower
5. 型付き IR からバイトコード生成

任意段階でエラーがあればコンパイル失敗とする。

## 3. AST 要件

AST ノードは次を保持する。

- ソース位置（`file`, `line`, `column`）
- 仕様書生成用メタデータ（ja/en 名称、説明、制約）
- 型情報スロット（後段で確定）

## 4. 型付き IR 要件

IR は以下を満たす。

- 糖衣構文を除去した明示表現
- 完全型付け済み
- 純粋性検査済み
- 評価順が決定的

最小命令要素:

- 定数
- 変数束縛/参照
- enum/record 構築
- match 分岐
- list/map 操作
- 関数呼び出し
- tuple/result 返却

## 5. バイトコード VM（主経路）

MVP の正準実行形式はバイトコードとする。

- MVP の正準抽象機械は**スタック方式**とする
- 命令意味は決定的であること
- 実行時 trap は構造化エラーに変換すること

### 5.0 MVP ブートストラップ（暫定）

初期実装段階では、正準経路（IR -> バイトコード -> VM）に加えて、
`cino-vm` が**型付き IR を直接評価する実行器**を提供してよい。

- 許可範囲は MVP の最小式（`LocalRef` / `Int` / `Bool` / `Tuple` / `Binary` / `Call` / `Let` / `Match`）
- `update/query` の公開契約、決定性、上限超過時エラー契約はバイトコード実行と同一
- panic/trap は構造化エラーに変換する
- 将来バイトコード経路が安定した時点で、IR直接実行器は開発用/検証用へ縮退してよい

### 5.1 抽象機械状態

実行状態は次の組で定義する。

- `pc`: 次に実行する命令位置
- `stack`: 値スタック（LIFO）
- `locals`: 現在フレームのローカル配列
- `call_stack`: 呼び出しフレーム列（`return_pc`, `locals`, `function_id`）
- `budget`: 残り実行ステップ予算

1 命令実行ごとに `budget` を 1 減算し、0 到達時は `E-RUNTIME-STEP-LIMIT` を返す。

### 5.2 MVP 命令セット

命令は opcode と固定長/可変長オペランドで表現する。以下が MVP 最小集合。

| 命令 | 入力（前提） | 出力（成功時） | 失敗条件 |
| --- | --- | --- | --- |
| `CONST k` | - | `stack.push(const_pool[k])` | `k` 範囲外 (`E-BC-INVALID-CONST`) |
| `LOAD_LOCAL i` | `locals[i]` が存在 | `stack.push(locals[i])` | `i` 範囲外 (`E-BC-INVALID-LOCAL`) |
| `STORE_LOCAL i` | `stack` 先頭に値 `v` | `locals[i] = v` | `stack` 空 / `i` 範囲外 |
| `MAKE_RECORD type_id, n` | `stack` に n 個のフィールド値 | `Record(type_id, fields)` を push | `n` 不正 / 型不一致 |
| `MAKE_ENUM tag_id, n` | `stack` に n 個の payload 値 | `Enum(tag_id, payload)` を push | `tag_id` 不正 / arity 不一致 |
| `LIST_NEW n` | `stack` に n 個の要素値 | `List` を push | `n` 不正 |
| `MAP_NEW n` | `stack` に `2n` 個（`k1,v1,...`） | `Map` を push | キー比較不能 / 重複キー / `n` 不正 |
| `GET_FIELD field_idx` | `stack` 先頭が record | フィールド値を push | 非 record / 範囲外 |
| `JUMP target` | - | `pc = target` | `target` 範囲外 (`E-BC-INVALID-JUMP`) |
| `JUMP_IF_FALSE target` | `stack` 先頭が `Bool` | 偽なら `pc = target` | 非 `Bool` / `target` 範囲外 |
| `MATCH_TAG {tag->target}` | `stack` 先頭が enum | 対応分岐へ `pc` 更新 | 非 enum / 未定義 tag |
| `CALL fn_id, argc` | 引数 `argc` 個が `stack` 先頭にある | 新フレームを push し呼び出し先へ遷移 | `fn_id` 不正 / arity 不一致 / 再帰上限超過 |
| `RETURN` | `stack` 先頭に戻り値 | 呼び出し元へ復帰し戻り値を push | フレーム不整合 / 空スタック |
| `TUPLE2` | `stack` 先頭に 2 値 | `(a, b)` を push | スタック不足 |
| `RESULT_OK` | `stack` 先頭に値 `v` | `Result::Ok(v)` を push | スタック不足 |
| `RESULT_ERR` | `stack` 先頭に値 `e` | `Result::Err(e)` を push | スタック不足 |

注記:

- `MAP_NEW` の重複キーは「最初に現れたキーを有効」にせず、**実行時エラー**として失敗させる。
- `MATCH_TAG` はコンパイル済み分岐表を参照し、線形探索を行わない（実装差による順序差を排除）。

### 5.3 決定性ルール

命令列生成と実行は以下を満たさなければならない。

- 式の評価順は常に**左から右**
- `record` フィールド評価順はソース定義順
- `List` 要素評価順はソース記述順
- `Map` の `key/value` 組はソース記述順で評価し、その順で構築
- `match` のアーム検査順はソース記述順（ただし実行は `MATCH_TAG` の直達ジャンプ）
- 関数引数評価順は左から右、`CALL` は評価後に一括でフレームへ束縛
- VM は実行中にホストコールバックしない（`Action` は値として構築のみ）

### 5.4 実行例（主要命令）

入力 cino（概念）:

```cino
update(state: S, event: E) -> (S, List<Action>) {
  match event {
    Tick {} => ({ count: state.count + 1 }, [Action.Notify])
  }
}
```

対応する命令列（概念）:

```text
LOAD_LOCAL 1                 ; event
MATCH_TAG { Tick -> L_tick }
L_tick:
LOAD_LOCAL 0                 ; state
GET_FIELD 0                  ; count
CONST 0                      ; 1
CALL add_int 2
MAKE_RECORD S 1
CONST 1                      ; Action.Notify
LIST_NEW 1
TUPLE2
RETURN
```

この例では `MATCH_TAG`, `GET_FIELD`, `MAKE_RECORD`, `LIST_NEW`, `TUPLE2`, `RETURN` の意味が固定される。

## 6. Rust ソース生成（任意）

Rust コード生成は次用途に限定して提供可能。

- デバッグ
- 可観測性
- オフライン検証

正準意味は IR + VM に置き、Rust生成物に依存しない。

## 7. 互換性

### 7.1 バイトコードヘッダ

全バイトコードは先頭に次ヘッダを持つ。

- `magic`: `CINOBC`（6 byte）
- `major`: u16
- `minor`: u16
- `flags`: u16（MVP では 0 固定）

### 7.2 バージョン規則

- `major`: 命令意味・エンコード・検証規則の非互換変更で増加
- `minor`: 後方互換な命令追加・メタ情報追加で増加
- `minor` 変更では既存命令の意味を変更してはならない

### 7.3 互換性判定

ランタイムは次の規則で受理/拒否する。

- `magic` 不一致は `E-BC-BAD-MAGIC`
- `major` 不一致は `E-BC-MAJOR-MISMATCH`
- `major` 一致かつ `bytecode.minor <= runtime.supported_minor` のときのみ実行可能
- 上記を満たさない場合、実行開始前に明示エラーで失敗する

### 7.4 非互換変更の例

次は必ず `major` を上げる。

- 既存 opcode の意味変更（例: `MAP_NEW` の重複キー規則を変更）
- オペランド解釈変更（例: `CALL` の引数順を変更）
- 既存 trap/error コード意味の変更

## 8. Rust ワークスペース構成（MVP 推奨）

MVP では Cargo workspace を採用し、責務境界ごとに次のクレートへ分割する。

1. `cino-syntax`
2. `cino-sema`
3. `cino-ir`
4. `cino-vm`
5. `cino-runtime`
6. `cino-ffi-c`
7. `cino-docgen`
8. `cino-cli`

初期段階では `cino-syntax` / `cino-sema` / `cino-ir` / `cino-vm` / `cino-cli` を最小セットとし、
FFI と docgen は段階的に追加してよい。

## 9. クレート依存方向

依存は次方向を原則とする。

- `cino-syntax` -> `cino-sema` -> `cino-ir` -> `cino-vm` -> `cino-runtime`
- `cino-ffi-c` は `cino-runtime` に依存
- `cino-cli` は上位オーケストレーションとして `cino-runtime` / `cino-docgen` に依存

循環依存は禁止する。
