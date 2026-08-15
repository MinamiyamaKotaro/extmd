# `reader::grid_builder` 設計書

対象: [reader/mod.md](mod.md)の対応表における `grid_builder.rs`。

## 1. 責務

散在するExcelセル情報から `(0, 0)` 起点の `rows × cols` 矩形領域を構築し、
値のないセルを `CellValue::Empty` で埋めた `domain::Grid<Cell>` を生成する
（[grid.md 2章](../domain/grid.md#2-構築方法ミュータブルapiを持たない理由)で
Grid側が要求する構築パターンに従う）。

## 2. 座標系の正規化

Gridの座標系はExcelシートの `(0, 0)`（A1セル）を起点として一貫して正規化する。
Excelシートの列幅や結合セル（`MergeRange`）は絶対座標で指定されているため、
データのBounding Boxの最小位置をオフセットして扱うと列幅の対応付けやインデックス
変換処理が複雑になりバグの温床となるため、常に `(0, 0)` を起点とする
（[Issue #4のコメント](https://github.com/MinamiyamaKotaro/extmd/issues/4#issuecomment-5301613143)の提案を反映）。

`umya-spreadsheet`の座標は1-based（A1 = `(col=1, row=1)`）であるため、
`domain::Grid`（0-based）へ変換する際は行・列とも `-1` するオフセット変換を行う。

## 3. 構築アルゴリズム

```rust
pub(crate) fn build_grid(
    ws: &umya_spreadsheet::Worksheet,
    rows: usize,
    cols: usize,
) -> domain::Grid<domain::Cell> {
    // 1. 列幅を先に解決する（列ごとに1回、cols回だけ umya-spreadsheet を問い合わせる）
    // column_dimension_by_numberは`col: u32`を値渡しで受け取る（&u32ではない）。
    let column_widths: Vec<f64> = (1..=cols as u32)
        .map(|col| ws.column_dimension_by_number(col)
            .map(|c| c.width())
            .unwrap_or(DEFAULT_COLUMN_WIDTH)) // Excel既定幅(8.38)
        .collect();

    // 2. CellValue::Empty相当のdomain::Cellでrows*cols件を初期化する
    let mut cells: Vec<domain::Cell> = (0..rows * cols)
        .map(|i| empty_cell(column_widths[i % cols]))
        .collect();

    // 3. 実際に値の存在するセルだけを走査し、該当インデックスを上書きする
    // Worksheetに`cell_collection()`というメソッドは存在しない。全セルを取得するには
    // `cells() -> Vec<&Cell>`を使う（3.1参照）。
    for excel_cell in ws.cells() {
        let (col, row) = (excel_cell.coordinate().col_num(), excel_cell.coordinate().row_num());
        if row == 0 || col == 0 || (row as usize) > rows || (col as usize) > cols {
            continue; // 3.1参照
        }
        let (r, c) = (row as usize - 1, col as usize - 1);
        cells[r * cols + c] = cell_mapper::map_cell(excel_cell, column_widths[c]);
    }

    // 4. Grid::newは最後に1回だけ呼ぶ（grid.md 2章の構築パターン）
    domain::Grid::new(rows, cols, cells)
}
```

### 3.1 走査対象セルの境界チェック

`Worksheet::cells()` はワークシートが内部的に保持する全セル（`Vec<&Cell>`）を返すが、
理論上 `highest_column_and_row()` で求めた範囲（`rows`/`cols`）を超えるセルは
存在しないはずである。ただし、umya-spreadsheet側の実装詳細に依存した前提を
Reader側で過信しないよう、範囲外インデックスへの書き込みで `cells` の境界を
超えないことを防御的にチェックする（[grid.md 4章](../domain/grid.md#4-境界チェックの方針)の
「外部入力を扱う場合は安全なAPI経由でアクセスする」という方針に準ずる）。

## 4. 列数0のシート（空シート）

[xlsx.md 3章](xlsx.md#3-列数0のシートの扱い)の通り、`rows=0, cols=1` が渡された場合、
上記アルゴリズムの手順1で `column_widths` は1要素（既定幅）、手順2で `cells` は
空の `Vec`（`0 * 1 = 0`件）となり、`Grid::new(0, 1, vec![])` が呼ばれる。
これは[grid.md 4章](../domain/grid.md#4-境界チェックの方針)が許容する
「`rows == 0` かつ `cols > 0`」の空シート表現と一致する。

## 5. 列幅未設定列のデフォルト値

Excelは列幅が明示的に設定されていない列に対して既定幅（`umya-spreadsheet`の
`Column`構造体の初期値は `8.38`）を使う。`column_dimension_by_number` が `None`
を返す（=その列の`ColumnDimension`情報自体が存在しない）場合は、この既定値
`DEFAULT_COLUMN_WIDTH = 8.38` にフォールバックする。

## 6. 計算量・メモリ使用量に関する留意点

`rows * cols` 件の `domain::Cell` を常に確保するため、Bounding Boxに対して
実データが疎（sparse）なシートではメモリ使用量が無駄に大きくなる可能性がある。
[非機能要件](../../requirement/requirements.md#6-非機能要件)が想定する規模
（数千〜数万セル）では許容範囲と判断するが、極端に疎な巨大シート
（例: `(1, 1)` と `(100000, 100000)` にだけ値がある等）は現実的なユースケースとして
想定しないため、上限チェック等は行わない。

## 7. 未確定事項

- `DEFAULT_COLUMN_WIDTH` の値（`umya-spreadsheet`のデフォルト値 `8.38` をそのまま
  使うか、要件定義書のヒューリスティック用に別途チューニングするか）
- 極端に疎なシートに対するメモリ使用量の実測（6章、実データでの検証が必要）
