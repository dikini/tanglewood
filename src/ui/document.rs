//! Native derived-document presentation. Source remains owned by the editor.
use crate::markdown::{BlockKind, Inline, RenderBlock};
use eframe::egui;

pub(super) fn render_block(ui: &mut egui::Ui, block: &RenderBlock, navigate: &mut Option<String>) {
    match &block.kind {
        BlockKind::Fence(fence) => {
            ui.monospace(&fence.body);
        }
        BlockKind::Source(source) => {
            ui.monospace(source);
        }
        BlockKind::Paragraph | BlockKind::Heading(_) => {
            ui.horizontal_wrapped(|ui| {
                let mut emphasis = false;
                let mut strong = matches!(block.kind, BlockKind::Heading(_));
                for inline in &block.inlines {
                    match inline {
                        Inline::Text(text) => {
                            let mut text = egui::RichText::new(text);
                            if emphasis {
                                text = text.italics();
                            }
                            if strong {
                                text = text.strong();
                            }
                            ui.label(text);
                        }
                        Inline::EmphasisStart => emphasis = true,
                        Inline::EmphasisEnd => emphasis = false,
                        Inline::StrongStart => strong = true,
                        Inline::StrongEnd => strong = matches!(block.kind, BlockKind::Heading(_)),
                        Inline::Code(text) => {
                            ui.monospace(text);
                        }
                        Inline::Link {
                            target,
                            label,
                            wiki,
                        } => {
                            if *wiki {
                                if ui.link(label).clicked() {
                                    *navigate = Some(target.clone());
                                }
                            } else {
                                if target.contains("://") || target.starts_with("mailto:") {
                                    ui.hyperlink_to(label, target);
                                } else if ui.link(label).clicked() {
                                    *navigate = Some(target.clone());
                                }
                            }
                        }
                        Inline::Break => {
                            ui.end_row();
                        }
                    }
                }
            });
        }
        BlockKind::Quote => {
            ui.group(|ui| {
                for child in &block.children {
                    render_block(ui, child, navigate);
                }
            });
        }
        BlockKind::Div => {
            for child in &block.children {
                render_block(ui, child, navigate);
            }
        }
    }
}

/// Pixels are decoded away from the event loop; only texture upload happens here.
#[cfg(feature = "documents")]
#[derive(Default)]
pub(super) struct ArtworkCache {
    generation: u64,
    active_generation: u64,
    queued: Option<Vec<(String, Vec<u8>, bool)>>,
    pending: Option<std::sync::mpsc::Receiver<Vec<(String, Result<egui::ColorImage, String>)>>>,
    images: std::collections::HashMap<String, Result<egui::TextureHandle, String>>,
}

#[cfg(feature = "documents")]
impl ArtworkCache {
    pub(super) fn prepare(
        &mut self,
        ctx: &egui::Context,
        generation: u64,
        artwork: Vec<(String, Vec<u8>, bool)>,
    ) {
        self.generation = generation;
        self.images.clear();
        self.queued = Some(artwork);
        if self.pending.is_some() {
            return;
        }
        self.start(ctx);
    }

    fn start(&mut self, ctx: &egui::Context) {
        let Some(artwork) = self.queued.take() else {
            return;
        };
        self.active_generation = self.generation;
        self.images.clear();
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.pending = Some(receiver);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut pixels = 0usize;
            let results = artwork.into_iter().map(|(id, bytes, svg)| {
                let result = if svg {
                    egui_extras::image::load_svg_bytes_with_size(
                        &bytes,
                        egui::SizeHint::Size { width: 1600, height: 1600, maintain_aspect_ratio: true },
                        &Default::default(),
                    )
                } else {
                    // Read dimensions before allocating the decoded PNG.
                    (|| {
                        let reader = image::ImageReader::with_format(std::io::Cursor::new(&bytes), image::ImageFormat::Png);
                        let (w, h) = reader.into_dimensions().map_err(|e| e.to_string())?;
                        if u64::from(w) * u64::from(h) > 4_000_000 {
                            return Err("Image exceeds preview pixel limit; source and original asset retained.".into());
                        }
                        egui_extras::image::load_image_bytes(&bytes).map_err(|e| e.to_string())
                    })()
                }.and_then(|image| {
                    pixels += image.pixels.len();
                    if pixels > 8_000_000 { Err("Document exceeds preview image memory limit.".into()) }
                    else { Ok(image) }
                });
                (id, result)
            }).collect();
            let _ = sender.send(results);
            ctx.request_repaint();
        });
    }

    pub(super) fn poll(&mut self, ctx: &egui::Context) {
        let Some(receiver) = &self.pending else {
            return;
        };
        match receiver.try_recv() {
            Ok(images) => {
                if self.active_generation != self.generation {
                    self.pending = None;
                    self.start(ctx);
                    return;
                }
                self.images = images
                    .into_iter()
                    .map(|(id, pixels)| {
                        let image = pixels.map(|pixels| {
                            ctx.load_texture(
                                format!("artwork-{}-{id}", self.generation),
                                pixels,
                                egui::TextureOptions::LINEAR,
                            )
                        });
                        (id, image)
                    })
                    .collect();
                self.pending = None;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                self.images
                    .insert(String::new(), Err("Image worker stopped.".into()));
                self.start(ctx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    pub(super) fn figure(&self, ui: &mut egui::Ui, figure: &crate::illustration::PreparedFigure) {
        ui.group(|ui| {
            match self.images.get(&figure.id) {
                Some(Ok(texture)) => {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(
                        texture.size_vec2()
                            * (ui.available_width().min(720.0) / texture.size_vec2().x).min(1.0),
                    ));
                }
                Some(Err(error)) => {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                None => {
                    ui.spinner();
                    ui.label("Preparing artwork…");
                }
            }
            ui.label(egui::RichText::new(&figure.caption).italics());
            if !figure.description.is_empty() {
                ui.label(&figure.description);
            }
        });
    }
}

/// A deliberately small Pandoc projection. Unknown nodes stay visible and read-only.
#[cfg(feature = "documents")]
pub(super) fn render_pandoc(
    ui: &mut egui::Ui,
    blocks: &[serde_json::Value],
    artwork: &ArtworkCache,
    figures: &[crate::illustration::PreparedFigure],
) {
    use serde_json::Value;
    fn inline_text(value: &Value) -> String {
        match value {
            Value::Array(values) => values.iter().map(inline_text).collect(),
            Value::Object(_) => match value["t"].as_str().unwrap_or("") {
                "Str" => value["c"].as_str().unwrap_or("").to_owned(),
                "Space" | "SoftBreak" => " ".into(),
                "LineBreak" => "\n".into(),
                "Code" | "Math" => value["c"][1].as_str().unwrap_or("").into(),
                "Link" | "Image" => inline_text(&value["c"][1]),
                "Span" | "Quoted" => inline_text(&value["c"][1]),
                _ => inline_text(&value["c"]),
            },
            _ => String::new(),
        }
    }
    for block in blocks {
        match block["t"].as_str().unwrap_or("") {
            "Para" | "Plain" => {
                ui.label(inline_text(&block["c"]));
            }
            "Header" => {
                ui.heading(inline_text(&block["c"][2]));
            }
            "CodeBlock" => {
                ui.monospace(block["c"][1].as_str().unwrap_or(""));
            }
            "Div" => {
                if let Some(children) = block["c"][1].as_array() {
                    render_pandoc(ui, children, artwork, figures);
                }
            }
            "BlockQuote" => {
                if let Some(children) = block["c"].as_array() {
                    ui.group(|ui| render_pandoc(ui, children, artwork, figures));
                }
            }
            "BulletList" | "OrderedList" => {
                let ordered = block["t"] == "OrderedList";
                let items = if ordered { &block["c"][1] } else { &block["c"] };
                if let Some(items) = items.as_array() {
                    for (index, item) in items.iter().enumerate() {
                        ui.horizontal_top(|ui| {
                            ui.label(if ordered {
                                format!("{}.", index + 1)
                            } else {
                                "•".into()
                            });
                            ui.vertical(|ui| {
                                if let Some(children) = item.as_array() {
                                    render_pandoc(ui, children, artwork, figures);
                                }
                            });
                        });
                    }
                }
            }
            "Figure" => {
                let id = block["c"][0][0].as_str().unwrap_or("");
                if let Some(figure) = figures.iter().find(|figure| figure.id == id) {
                    artwork.figure(ui, figure);
                } else {
                    ui.label("Figure has no verified local artwork; use source mode.");
                }
            }
            "HorizontalRule" => {
                ui.separator();
            }
            _ => {
                ui.collapsing(
                    format!(
                        "{} — source view available",
                        block["t"].as_str().unwrap_or("Unsupported block")
                    ),
                    |ui| {
                        ui.monospace(block.to_string());
                    },
                );
            }
        }
        ui.add_space(6.0);
    }
}
