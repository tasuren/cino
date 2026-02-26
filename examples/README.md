# cino examples

## 小さいサンプル（`.cino` 単体）

| ファイル | 内容 | ポイント |
|---|---|---|
| `counter.cino` | カウンター（増減・リセット） | State/Event/update/query の最小構成 |
| `traffic_light.cino` | 信号機（Red→Green→Yellow→Red…） | enum を状態として使う状態機械 |
| `todo_list.cino` | 未完了タスク数管理 | タスク完了時に `Action` を生成するデモ |

## 大きめのサンプル

| ファイル | 内容 | ポイント |
|---|---|---|
| `shopping_cart.cino` | ショッピングカート（合計管理・注文） | ユーザー定義関数 `fn`、複数 Action、複数 Event を網羅 |

## Rust インテグレーションサンプル

### `counter_app/`

`counter.cino` を cino-runtime 経由でロードし、libui で GUI を表示するカウンターアプリです。

```
examples/counter_app/
  Cargo.toml    # libui + cino クレートの依存
  counter.cino  # cino ソース（バイナリに埋め込まれる）
  src/
    main.rs     # Rust アプリ本体
```

#### 必要な環境

| OS | 依存 |
|---|---|
| macOS | Xcode Command Line Tools（Cocoa が自動利用） |
| Linux | GTK3（例: `sudo apt install libgtk-3-dev`） |
| Windows | Win32 API（自動） |

#### 実行

```sh
cd examples/counter_app
cargo run
```

## cino CLI での動作確認

```sh
# ビルド
cargo build -p cino-cli --bin cino

# check
./target/debug/cino check --file examples/counter.cino

# update 実行
./target/debug/cino run update \
  --file examples/counter.cino \
  --state 0 \
  --event '{"$tag":"Increment","$fields":{}}'

# query 実行
./target/debug/cino run query \
  --file examples/counter.cino \
  --state 5 \
  --query '{"$tag":"GetCount","$fields":{}}'
```
