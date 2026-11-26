use statsapi_vdem::{extract_archive, latest};
use std::io::Write;

#[test]
fn extracts_synthetic_archive_and_selects_latest_year() {
    let mut archive = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    archive
        .start_file("vdem.csv", zip::write::SimpleFileOptions::default())
        .expect("fixture member starts");
    archive
        .write_all(
            b"country_text_id,year,v2x_libdem,v2x_polyarchy,v2x_freexp_altinf,v2x_cspart,v2x_corr\nUSA,2020,0.4,0.5,0.6,0.7,0.8\nUSA,2021,0.9,0.8,0.7,0.6,0.5\n",
        )
        .expect("fixture writes");
    let bytes = archive.finish().expect("fixture closes").into_inner();
    let extract = extract_archive(std::io::Cursor::new(bytes)).expect("archive extracts");
    assert_eq!(
        latest(&extract, "USA", "v2x_libdem").expect("CSV parses"),
        Some((0.9, "2021".into()))
    );
}
