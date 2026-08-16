use std::path::Path;

fn build_bench_sheet(rows: u32, cols: u32, path: &Path) {
    let mut book = umya_spreadsheet::new_file();
    let ws = book.sheet_by_name_mut("Sheet1").unwrap();
    ws.set_name("bench");

    // Set column widths
    for col in 1..=cols {
        ws.column_dimension_by_number_mut(col).set_width(10.0);
    }

    // Fill cells
    for r in 1..=rows {
        for c in 1..=cols {
            let cell = ws.cell_mut((c, r));
            if c % 2 == 0 {
                cell.set_value_number(12.34);
                cell.style_mut()
                    .numbering_format_mut()
                    .set_format_code("0.00");
            } else {
                cell.set_value(format!("row{r}_col{c}"));
            }
        }
    }

    umya_spreadsheet::writer::xlsx::write(&book, path).unwrap();
    println!(
        "Generated: {} ({} x {} = {} cells)",
        path.display(),
        rows,
        cols,
        rows * cols
    );
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    std::fs::create_dir_all(&dir).unwrap();

    // 10k cells (100 rows x 100 cols)
    build_bench_sheet(100, 100, &dir.join("bench_10k.xlsx"));

    // 50k cells (500 rows x 100 cols)
    build_bench_sheet(500, 100, &dir.join("bench_50k.xlsx"));

    // 100k cells (500 rows x 200 cols)
    build_bench_sheet(500, 200, &dir.join("bench_100k.xlsx"));

    // 500k cells (1000 rows x 500 cols)
    build_bench_sheet(1000, 500, &dir.join("bench_500k.xlsx"));

    // 1M cells (1000 rows x 1000 cols)
    build_bench_sheet(1000, 1000, &dir.join("bench_1m.xlsx"));
}
