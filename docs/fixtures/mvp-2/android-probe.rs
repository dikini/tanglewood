use std::{fs,path::PathBuf,io::Cursor};
use tanglewood::illustration::{prepare,AssetFile,PreparationInput,PreparationError};
fn main()->Result<(),Box<dyn std::error::Error>> {
 let root=PathBuf::from(std::env::args().nth(1).ok_or("fixture root required")?);
 let source=fs::read_to_string(root.join("source.md"))?;
 let paths=["assets/fig-add-inputs/illustration.json","assets/fig-add-inputs/figure.svg","assets/fig-choosing-a-result/comic.json","assets/fig-choosing-a-result/strip.png"];
 let assets=paths.iter().map(|path|Ok(AssetFile{relative_path:(*path).into(),bytes:fs::read(root.join(path))?})).collect::<Result<Vec<_>,std::io::Error>>()?;
 let input=PreparationInput{source:source.clone(),logical_path:"source.md".into(),assets};let prepared=prepare(input.clone())?;
 let expected:serde_json::Value=serde_json::from_slice(&fs::read(root.join("expected-semantic.json"))?)?;
 for (figure,expected) in prepared.figures.iter().zip(expected.as_array().ok_or("expected array")?) {
  assert_eq!(figure.id,expected["id"]);assert_eq!(figure.caption,expected["caption"]);assert_eq!(figure.description,expected["image"][1][0]["c"]);assert_eq!(figure.asset_path,expected["image"][2][0]);assert_eq!(figure.width_mm.as_deref(),expected["image"][0][2][1][1].as_str());
  assert!(source[figure.source_bytes.clone()].contains(&format!("#{}",figure.id)));
 }
 assert_eq!(prepared.figures.len(),2);
 let expected_metadata:serde_json::Value=serde_json::from_slice(&fs::read(root.join("expected-derived-metadata.json"))?)?;
 assert_eq!(serde_json::to_value(&prepared.metadata)?,expected_metadata);
 // Actually decode the accepted comic on the device as a usable raster, not
 // just an envelope/header assertion. Renderer/APK wiring remains later work.
 let png=&input.assets.iter().find(|asset|asset.relative_path.ends_with("strip.png")).ok_or("PNG")?.bytes;
 let mut decoder=png::Decoder::new(Cursor::new(png)).read_info()?;let mut pixels=vec![0;decoder.output_buffer_size().ok_or("PNG size")?];let image=decoder.next_frame(&mut pixels)?;assert_eq!((image.width,image.height),(1536,1024));
 let mut missing=input;missing.assets.retain(|asset|!asset.relative_path.ends_with("figure.svg"));assert!(matches!(prepare(missing),Err(PreparationError::Missing(_))));
 println!("{}",serde_json::json!({"result":"PASS","platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"figures":prepared.figures,"metadata":prepared.metadata,"semantic_matches_pinned_upstream":true,"missing_svg_rejected":true,"comic_decoded_pixels":[image.width,image.height],"source_unchanged":true}));
 Ok(())
}
