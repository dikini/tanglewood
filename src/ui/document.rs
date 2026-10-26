//! Quiet native document presentation. Editing always targets original source.
use crate::markdown::{BlockKind, Inline};
use eframe::egui;
use std::ops::Range;

pub(super) fn render_node(
    ui: &mut egui::Ui,
    document: &crate::document_world::DocumentWorld,
    entity: bevy_ecs::entity::Entity,
    navigate: &mut Option<String>,
) {
    let Some(block) = document.node(entity) else {
        return;
    };
    match block.kind {
        BlockKind::Fence(fence)
            if fence
                .attributes
                .classes
                .iter()
                .any(|class| class == "callout") =>
        {
            quote(ui, |ui| {
                prose(
                    ui,
                    &[Inline::Text(fence.body.trim().into())],
                    None,
                    navigate,
                )
            });
        }
        BlockKind::Fence(fence)
            if fence
                .attributes
                .classes
                .iter()
                .any(|class| matches!(class.as_str(), "illustration" | "comic")) =>
        {
            ui.weak("Artwork unavailable");
            ui.add_space(29.0);
        }
        BlockKind::Fence(fence) => code(ui, &fence.body),
        BlockKind::Source(source) => render_source(ui, source, navigate),
        BlockKind::Paragraph => prose(ui, block.inlines, None, navigate),
        BlockKind::Heading(level) => prose(ui, block.inlines, Some(*level), navigate),
        BlockKind::Quote => quote(ui, |ui| {
            for child in block.children {
                render_node(ui, document, *child, navigate);
            }
        }),
        BlockKind::Div => {
            for child in block.children {
                render_node(ui, document, *child, navigate);
            }
        }
    }
}

fn prose(
    ui: &mut egui::Ui,
    inlines: &[Inline],
    heading: Option<u8>,
    navigate: &mut Option<String>,
) {
    let size = match heading {
        Some(1) => 28.0,
        Some(2) => 23.0,
        Some(_) => 19.0,
        None => 15.5,
    };
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = ui.available_width().max(1.0);
    let mut emphasis = false;
    let mut strong = heading.is_some();
    let mut links = Vec::new();
    let mut code_spans = Vec::new();
    let mut chars = 0;
    for inline in inlines {
        let (text, mono, link) = match inline {
            Inline::Text(text) => (text.as_str(), false, None),
            Inline::Code(text) => (text.as_str(), true, None),
            Inline::Break => ("\n", false, None),
            Inline::EmphasisStart => {
                emphasis = true;
                continue;
            }
            Inline::EmphasisEnd => {
                emphasis = false;
                continue;
            }
            Inline::StrongStart => {
                strong = true;
                continue;
            }
            Inline::StrongEnd => {
                strong = heading.is_some();
                continue;
            }
            Inline::Link {
                target,
                label,
                wiki,
            } => (label.as_str(), false, Some((target, *wiki))),
        };
        let color = if strong {
            ui.visuals().strong_text_color()
        } else {
            ui.visuals().text_color()
        };
        let mut format = egui::TextFormat {
            font_id: if mono {
                egui::FontId::monospace(size - 1.0)
            } else {
                egui::FontId::proportional(size)
            },
            color,
            italics: emphasis,
            line_height: Some(if heading.is_some() { size * 1.3 } else { 27.0 }),
            ..Default::default()
        };
        if let Some((target, wiki)) = link {
            format.underline = egui::Stroke::new(0.6, ui.visuals().weak_text_color());
            links.push((chars..chars + text.chars().count(), target.clone(), wiki));
        }
        if mono {
            // Reserve actual horizontal padding so the background cannot collide
            // with neighbouring prose. These spaces belong only to the view.
            let padding = egui::TextFormat {
                font_id: egui::FontId::proportional(size),
                ..format.clone()
            };
            let start = chars;
            job.append("\u{2009}", 0.0, padding.clone());
            job.append(text, 0.0, format);
            job.append("\u{2009}", 0.0, padding);
            chars += text.chars().count() + 2;
            code_spans.push(start..chars);
        } else {
            chars += text.chars().count();
            job.append(text, 0.0, format);
        }
    }
    let plain = job.text.clone();
    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    let sense = if links.is_empty() {
        egui::Sense::hover()
    } else {
        egui::Sense::click()
    };
    let (rect, response) = ui.allocate_exact_size(galley.size(), sense);
    let code_fill = egui::Color32::from_gray(if ui.visuals().dark_mode { 38 } else { 232 });
    for highlight in code_highlights(&galley, &code_spans) {
        ui.painter()
            .rect_filled(highlight.translate(rect.min.to_vec2()), 4.0, code_fill);
    }
    ui.painter()
        .galley(rect.min, galley.clone(), ui.visuals().text_color());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &plain));
    if let Some(pos) = response.hover_pos() {
        let cursor = galley.cursor_from_pos(pos - rect.min).index;
        if let Some((_, target, wiki)) = links.iter().find(|(range, _, _)| range.contains(&cursor))
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            if response.clicked() {
                if !wiki && (target.contains("://") || target.starts_with("mailto:")) {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(target));
                } else {
                    *navigate = Some(target.clone());
                }
            }
        }
    }
    if !links.is_empty() {
        // Real link widgets retain independent keyboard and accessibility actions;
        // the paragraph above remains one coherent galley rather than token widgets.
        ui.menu_button(
            egui::RichText::new("Links")
                .size(11.0)
                .color(ui.visuals().weak_text_color()),
            |ui| {
                for (range, target, wiki) in &links {
                    let label: String = plain.chars().skip(range.start).take(range.len()).collect();
                    if !wiki && (target.contains("://") || target.starts_with("mailto:")) {
                        ui.hyperlink_to(label, target);
                    } else if ui.link(label).clicked() {
                        *navigate = Some(target.clone());
                        ui.close();
                    }
                }
            },
        )
        .response
        .on_hover_text("Links in this paragraph");
    }
    ui.add_space(if heading.is_some() { 17.0 } else { 18.0 });
}

/// Inline code backgrounds follow font metrics, not the paragraph's line box.
/// Splitting at row boundaries also handles long code spans wrapping in prose.
fn code_highlights(galley: &egui::Galley, spans: &[Range<usize>]) -> Vec<egui::Rect> {
    let mut highlights = Vec::new();
    let mut offset = 0;
    for row in &galley.rows {
        let end = offset + row.glyphs.len();
        for span in spans {
            let start = span.start.max(offset);
            let stop = span.end.min(end);
            if start >= stop {
                continue;
            }
            let glyphs = &row.glyphs[start - offset..stop - offset];
            let top = glyphs
                .iter()
                .map(|g| g.pos.y - g.font_ascent)
                .fold(f32::INFINITY, f32::min);
            let bottom = glyphs
                .iter()
                .map(|g| g.pos.y - g.font_ascent + g.font_height)
                .fold(f32::NEG_INFINITY, f32::max);
            highlights.push(egui::Rect::from_min_max(
                row.pos + egui::vec2(glyphs[0].pos.x, top - 1.0),
                row.pos + egui::vec2(glyphs.last().unwrap().max_x(), bottom + 1.0),
            ));
        }
        offset = end + usize::from(row.ends_with_newline);
    }
    highlights
}
fn code(ui: &mut egui::Ui, source: &str) {
    ui.add(egui::Label::new(egui::RichText::new(source).monospace().size(13.0)).wrap());
    ui.add_space(18.0);
}
fn quote(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    let response = ui
        .horizontal_top(|ui| {
            ui.add_space(17.0);
            ui.vertical(|ui| {
                ui.visuals_mut().override_text_color = Some(ui.visuals().weak_text_color());
                content(ui);
            });
        })
        .response;
    let rect = response.rect;
    ui.painter().line_segment(
        [rect.left_top(), rect.left_bottom() - egui::vec2(0.0, 15.0)],
        egui::Stroke::new(2.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    ui.add_space(8.0);
}
fn render_source(ui: &mut egui::Ui, source: &str, _navigate: &mut Option<String>) {
    // Preserve the existing reader's unsupported structures as original source.
    code(ui, source);
}

/// Reserve a quiet gutter, keeping the pencil keyboard reachable even before hover.
pub(super) fn editable_block(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    source: Option<Range<usize>>,
    render: impl FnOnce(&mut egui::Ui),
) -> Option<Range<usize>> {
    let focus_id = super::navigation::block_id(&id);
    let mut clicked = false;
    let touch = cfg!(target_os = "android") || ui.input(|input| input.has_touch_screen());
    let size = if touch { 44.0 } else { 28.0 };
    let width = ui.available_width();
    ui.push_id(id, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let content = ui.allocate_ui_with_layout(
                egui::vec2((width - size - 4.0).max(1.0), 0.0),
                egui::Layout::top_down(egui::Align::Min),
                render,
            );
            let rect = ui.allocate_space(egui::vec2(size, size)).1;
            let response = ui.interact(
                rect,
                focus_id,
                if source.is_some() {
                    egui::Sense::click()
                } else {
                    egui::Sense::hover()
                },
            );
            if source.is_some() {
                super::navigation::lock_arrows(&response);
                if response.gained_focus() {
                    ui.scroll_to_rect(content.response.rect, None);
                }
                if response.has_focus() {
                    let offset = egui::vec2(-10.0, 0.0);
                    let top = content.response.rect.left_top() + offset;
                    let bottom = content.response.rect.left_bottom() + offset;
                    ui.painter()
                        .with_clip_rect(ui.clip_rect().expand2(egui::vec2(12.0, 0.0)))
                        .line_segment(
                            [top, bottom],
                            egui::Stroke::new(
                                2.0,
                                ui.visuals().weak_text_color().gamma_multiply(0.6),
                            ),
                        );
                }
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Edit block source")
                });
                let visible = touch || content.response.contains_pointer() || response.hovered();
                if visible {
                    let color = if response.hovered() || response.has_focus() {
                        ui.visuals().text_color()
                    } else {
                        ui.visuals().weak_text_color()
                    };
                    let center = rect.center();
                    let stroke = egui::Stroke::new(1.4, color);
                    let points = [
                        center + egui::vec2(-6.0, 6.0),
                        center + egui::vec2(-4.0, 1.0),
                        center + egui::vec2(4.0, -7.0),
                        center + egui::vec2(7.0, -4.0),
                        center + egui::vec2(-1.0, 4.0),
                        center + egui::vec2(-6.0, 6.0),
                    ];
                    ui.painter().add(egui::Shape::line(points.to_vec(), stroke));
                }
                clicked = response.clicked();
                response.on_hover_text("Edit block source · Enter\nPrevious/next block · ↑/↓");
            }
        });
    });
    if clicked { source } else { None }
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
        _generation: u64,
        artwork: Vec<(String, Vec<u8>, bool)>,
    ) {
        self.invalidate();
        self.queued = Some(artwork);
        if self.pending.is_some() {
            return;
        }
        self.start(ctx);
    }

    pub(super) fn invalidate(&mut self) {
        self.generation = self
            .generation
            .checked_add(1)
            .expect("artwork generation exhausted");
        self.images.clear();
        self.queued = None;
    }

    pub(super) fn is_busy(&self) -> bool {
        self.pending.is_some() || self.queued.is_some()
    }
    pub(super) fn error(&self) -> Option<&str> {
        self.images
            .values()
            .find_map(|image| image.as_ref().err().map(String::as_str))
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
                if self.active_generation == self.generation {
                    self.images
                        .insert(String::new(), Err("Image worker stopped.".into()));
                }
                self.start(ctx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }

    pub(super) fn figure(&self, ui: &mut egui::Ui, figure: &crate::illustration::PreparedFigure) {
        match self.images.get(&figure.id) {
            Some(Ok(texture)) => {
                let size = texture.size_vec2();
                let width = figure
                    .width_mm
                    .as_ref()
                    .and_then(|width| {
                        width
                            .strip_suffix("mm")
                            .unwrap_or(width)
                            .trim()
                            .parse::<f32>()
                            .ok()
                    })
                    .map(|mm| mm * 96.0 / 25.4)
                    .unwrap_or(size.x);
                let width = width.min(ui.available_width()).min(size.x);
                ui.vertical_centered(|ui| {
                    let mut image =
                        egui::Image::new(texture).fit_to_exact_size(size * (width / size.x));
                    if matches!(figure.kind, crate::illustration::ArtworkKind::Illustration) {
                        // Saved SVGs use transparent paper with source-defined dark strokes.
                        image = image.bg_fill(egui::Color32::from_rgb(250, 250, 248));
                    }
                    let response = ui.add(image);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Image,
                            true,
                            &figure.description,
                        )
                    });
                    response.on_hover_text(&figure.description);
                });
            }
            Some(Err(_)) => {
                ui.weak("Artwork unavailable");
            }
            None => {
                ui.add_space(28.0);
            }
        }
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(&figure.caption)
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                )
                .wrap(),
            );
        });
        ui.add_space(29.0);
    }
}

/// A deliberately small Pandoc projection. Arbitrary transformed text is never bound.
#[cfg(feature = "documents")]
pub(super) fn render_pandoc(
    ui: &mut egui::Ui,
    blocks: &[serde_json::Value],
    artwork: &ArtworkCache,
    figures: &[crate::illustration::PreparedFigure],
    source: &str,
    editable: bool,
    navigate: &mut Option<String>,
) -> Option<Range<usize>> {
    fn render(
        ui: &mut egui::Ui,
        blocks: &[serde_json::Value],
        counts: &std::collections::BTreeMap<String, usize>,
        artwork: &ArtworkCache,
        figures: &[crate::illustration::PreparedFigure],
        source: &str,
        editable: bool,
        navigate: &mut Option<String>,
        selected: &mut Option<Range<usize>>,
    ) {
        for (index, block) in blocks.iter().enumerate() {
            ui.push_id(index, |ui| match block["t"].as_str().unwrap_or("") {
                "Para" | "Plain" => prose(ui, &pandoc_inlines(&block["c"]), None, navigate),
                "Header" => prose(
                    ui,
                    &pandoc_inlines(&block["c"][2]),
                    Some(block["c"][0].as_u64().unwrap_or(1) as u8),
                    navigate,
                ),
                "CodeBlock" => code(ui, block["c"][1].as_str().unwrap_or("")),
                "Div" => {
                    if let Some(children) = block["c"][1].as_array() {
                        render(
                            ui, children, counts, artwork, figures, source, editable, navigate,
                            selected,
                        );
                    }
                }
                "BlockQuote" => {
                    if let Some(children) = block["c"].as_array() {
                        quote(ui, |ui| {
                            render(
                                ui, children, counts, artwork, figures, source, editable, navigate,
                                selected,
                            )
                        });
                    }
                }
                "BulletList" | "OrderedList" => {
                    let ordered = block["t"] == "OrderedList";
                    let items = if ordered { &block["c"][1] } else { &block["c"] };
                    if let Some(items) = items.as_array() {
                        for (index, item) in items.iter().enumerate() {
                            ui.horizontal_top(|ui| {
                                ui.label(
                                    egui::RichText::new(if ordered {
                                        format!("{}.", index + 1)
                                    } else {
                                        "•".into()
                                    })
                                    .size(15.5),
                                );
                                ui.vertical(|ui| {
                                    if let Some(children) = item.as_array() {
                                        render(
                                            ui, children, counts, artwork, figures, source,
                                            editable, navigate, selected,
                                        );
                                    }
                                });
                            });
                        }
                    }
                }
                "Figure" => {
                    if let Some(figure) = verified_pandoc_figure(block, counts, figures, source) {
                        let range = editable.then(|| figure.source_bytes.clone());
                        if let Some(range) =
                            editable_block(ui, ("figure", &figure.id), range, |ui| {
                                artwork.figure(ui, figure)
                            })
                        {
                            *selected = Some(range);
                        }
                    } else {
                        ui.weak("Artwork unavailable")
                            .on_hover_text("Open document source or refresh preview.");
                    }
                }
                "HorizontalRule" => {
                    ui.separator();
                    ui.add_space(18.0);
                }
                _ => {
                    ui.weak("Unsupported content")
                        .on_hover_text("Available in the document source outline.");
                }
            });
        }
    }
    let mut selected = None;
    render(
        ui,
        blocks,
        &figure_counts(blocks),
        artwork,
        figures,
        source,
        editable,
        navigate,
        &mut selected,
    );
    selected
}

#[cfg(feature = "documents")]
fn pandoc_inlines(value: &serde_json::Value) -> Vec<Inline> {
    fn collect(value: &serde_json::Value, result: &mut Vec<Inline>) {
        if let Some(values) = value.as_array() {
            for value in values {
                collect(value, result);
            }
            return;
        }
        if !value.is_object() {
            return;
        }
        match value["t"].as_str().unwrap_or("") {
            "Str" => result.push(Inline::Text(value["c"].as_str().unwrap_or("").into())),
            "Space" | "SoftBreak" => result.push(Inline::Text(" ".into())),
            "LineBreak" => result.push(Inline::Break),
            "Code" | "Math" => {
                result.push(Inline::Code(value["c"][1].as_str().unwrap_or("").into()))
            }
            "Emph" => {
                result.push(Inline::EmphasisStart);
                collect(&value["c"], result);
                result.push(Inline::EmphasisEnd);
            }
            "Strong" => {
                result.push(Inline::StrongStart);
                collect(&value["c"], result);
                result.push(Inline::StrongEnd);
            }
            "Link" => result.push(Inline::Link {
                target: value["c"][2][0].as_str().unwrap_or("").into(),
                label: inline_text(&value["c"][1]),
                wiki: false,
            }),
            "Image" | "Span" | "Quoted" => collect(&value["c"][1], result),
            _ => collect(&value["c"], result),
        }
    }
    let mut result = Vec::new();
    collect(value, &mut result);
    result
}
#[cfg(feature = "documents")]
fn inline_text(value: &serde_json::Value) -> String {
    pandoc_inlines(value)
        .iter()
        .filter_map(|inline| match inline {
            Inline::Text(text) | Inline::Code(text) => Some(text.as_str()),
            Inline::Break => Some("\n"),
            Inline::Link { label, .. } => Some(label.as_str()),
            _ => None,
        })
        .collect()
}

/// Only the unique, unchanged output of the supported artwork adapter gets its span.
#[cfg(feature = "documents")]
fn verified_pandoc_figure<'a>(
    block: &serde_json::Value,
    counts: &std::collections::BTreeMap<String, usize>,
    figures: &'a [crate::illustration::PreparedFigure],
    source: &str,
) -> Option<&'a crate::illustration::PreparedFigure> {
    let id = block["c"][0][0].as_str()?;
    let mut matching = figures.iter().filter(|figure| figure.id == id);
    let figure = matching.next()?;
    if matching.next().is_some() || counts.get(id) != Some(&1) {
        return None;
    }
    let original = source.get(figure.source_bytes.clone())?;
    let original_id = pulldown_cmark::Parser::new(original).find_map(|event| match event {
        pulldown_cmark::Event::Start(pulldown_cmark::Tag::CodeBlock(
            pulldown_cmark::CodeBlockKind::Fenced(info),
        )) => crate::markdown::fence_attributes(&info).and_then(|attributes| attributes.id),
        _ => None,
    });
    if original_id.as_deref() != Some(id) {
        return None;
    }
    if block["c"][2].as_array()?.len() != 1
        || block["c"][2][0]["t"] != "Plain"
        || block["c"][2][0]["c"].as_array()?.len() != 1
    {
        return None;
    }
    let caption = inline_text(&block["c"][1][1]);
    let image = &block["c"][2][0]["c"][0];
    if caption != figure.caption
        || image["t"] != "Image"
        || image["c"][2][0].as_str()? != figure.asset_path
        || inline_text(&image["c"][1]) != figure.description
    {
        return None;
    }
    Some(figure)
}

#[cfg(feature = "documents")]
fn figure_counts(blocks: &[serde_json::Value]) -> std::collections::BTreeMap<String, usize> {
    fn visit(value: &serde_json::Value, counts: &mut std::collections::BTreeMap<String, usize>) {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    visit(value, counts);
                }
            }
            serde_json::Value::Object(values) => {
                if value["t"] == "Figure" {
                    if let Some(id) = value["c"][0][0].as_str() {
                        *counts.entry(id.into()).or_default() += 1;
                    }
                }
                for value in values.values() {
                    visit(value, counts);
                }
            }
            _ => {}
        }
    }
    let mut counts = std::collections::BTreeMap::new();
    for block in blocks {
        visit(block, &mut counts);
    }
    counts
}

#[cfg(feature = "documents")]
pub(super) fn pandoc_diagnostic(
    blocks: &[serde_json::Value],
    figures: &[crate::illustration::PreparedFigure],
    source: &str,
) -> Option<String> {
    fn visit(
        blocks: &[serde_json::Value],
        counts: &std::collections::BTreeMap<String, usize>,
        figures: &[crate::illustration::PreparedFigure],
        source: &str,
    ) -> Option<String> {
        for block in blocks {
            let tag = block["t"].as_str().unwrap_or("Unknown");
            let children = match tag {
                "Figure" if verified_pandoc_figure(block, counts, figures, source).is_none() => return Some("A figure does not match verified local artwork. Open source or review the processing settings, then refresh.".into()),
                "Div" => block["c"][1].as_array(),
                "BlockQuote" => block["c"].as_array(),
                "BulletList" | "OrderedList" => {
                    let items = if tag == "OrderedList" { &block["c"][1] } else { &block["c"] };
                    if let Some(items) = items.as_array() { for item in items { if let Some(children) = item.as_array() { if let Some(error) = visit(children, counts, figures, source) { return Some(error); } } } }
                    None
                },
                "Para" | "Plain" | "Header" | "CodeBlock" | "Figure" | "HorizontalRule" => None,
                _ => return Some(format!("The preview does not yet display {tag} content. It remains available in the source outline.")),
            };
            if let Some(children) = children {
                if let Some(error) = visit(children, counts, figures, source) {
                    return Some(error);
                }
            }
        }
        None
    }
    visit(blocks, &figure_counts(blocks), figures, source)
}

#[cfg(all(test, feature = "documents"))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_unique_unchanged_adapter_figures_bind_and_old_pixels_are_discarded() {
        let source = "```{#fig-proof .illustration}\nsaved artwork\n```\n";
        let figure = crate::illustration::PreparedFigure {
            id: "fig-proof".into(),
            kind: crate::illustration::ArtworkKind::Illustration,
            caption: "Proof".into(),
            description: "Alt".into(),
            width_mm: None,
            source_bytes: 0..source.len(),
            asset_path: "assets/fig-proof/figure.svg".into(),
            asset_sha256: String::new(),
            dimensions: [100.0, 100.0],
        };
        let block = json!({"t":"Figure", "c":[["fig-proof",[],[]], [null,[{"t":"Plain","c":[{"t":"Str","c":"Proof"}]}]], [{"t":"Plain","c":[{"t":"Image","c":[["",[],[]],[{"t":"Str","c":"Alt"}],["assets/fig-proof/figure.svg",""]]}]}]]});
        let figures = vec![figure];
        let blocks = vec![block.clone()];
        assert!(
            verified_pandoc_figure(&block, &figure_counts(&blocks), &figures, source).is_some()
        );
        let duplicated = vec![block.clone(), block.clone()];
        assert!(
            verified_pandoc_figure(&block, &figure_counts(&duplicated), &figures, source).is_none()
        );
        let mut rewritten = block.clone();
        rewritten["c"][2][0]["c"][0]["c"][2][0] = json!("other.svg");
        assert!(
            verified_pandoc_figure(
                &rewritten,
                &figure_counts(&[rewritten.clone()]),
                &figures,
                source
            )
            .is_none()
        );
        assert!(
            verified_pandoc_figure(
                &block,
                &figure_counts(&blocks),
                &figures,
                "different source"
            )
            .is_none()
        );

        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        sender
            .send(vec![(
                "fig-proof".into(),
                Ok(egui::ColorImage::new([1, 1], vec![egui::Color32::RED])),
            )])
            .unwrap();
        let mut cache = ArtworkCache {
            generation: 1,
            active_generation: 1,
            pending: Some(receiver),
            ..Default::default()
        };
        cache.invalidate();
        cache.poll(&egui::Context::default());
        assert!(cache.images.is_empty());
        assert!(!cache.is_busy());
    }
}
