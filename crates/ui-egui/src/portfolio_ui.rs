//! Portfolio dialog: turn the document into a PDF Portfolio (catalog `/Collection`).

use crate::theme::{self, Tokens};
use crate::PdfCraftApp;
use pdfcraft_engine::{CollectionField, PortfolioView};

pub struct PortfolioDraft {
    pub title: String,
    pub view: PortfolioView,
    pub fields: Vec<CollectionField>,
    pub new_field_name: String,
    pub new_field_source: String,
}

impl Default for PortfolioDraft {
    fn default() -> Self {
        Self {
            title: String::new(),
            view: PortfolioView::Tile,
            fields: Vec::new(),
            new_field_name: String::new(),
            new_field_source: "Name".to_string(),
        }
    }
}

/// The dialog body. Returns `(create, clear)` flags.
pub(crate) fn body(
    ui: &mut egui::Ui,
    app: &mut PdfCraftApp,
    t: &Tokens,
) -> (bool, bool, bool, String, PortfolioView, Vec<CollectionField>) {
    // Read the current attachments before borrowing the draft.
    let attachments: Vec<(String, Option<usize>)> = app
        .active_ids()
        .and_then(|(_, id)| app.session.get(id))
        .map(|doc| doc.info.attachments.iter().map(|a| (a.name.clone(), a.size)).collect())
        .unwrap_or_default();

    let Some(d) = app.portfolio_draft.as_mut() else {
        return (false, false, false, String::new(), PortfolioView::Tile, Vec::new());
    };
    let mut add_file = false;
    ui.label(egui::RichText::new(tl!("Create PDF Portfolio")).font(theme::semibold(18.0)));
    ui.add_space(8.0);
    ui.label(tl!("A portfolio shows the document's embedded files (attachments) in a side panel instead of a plain list."));
    ui.add_space(8.0);

    // Current embedded files (portfolio members).
    if !attachments.is_empty() {
        ui.label(tl!("Files in portfolio"));
        ui.add_space(4.0);
        for (name, size) in &attachments {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(name).font(theme::medium(12.0)));
                ui.label(
                    egui::RichText::new(size.map(crate::panels::human_size).unwrap_or_default())
                        .color(t.text_faint)
                        .small(),
                );
            });
        }
        ui.add_space(8.0);
    }

    // Add to portfolio.
    ui.horizontal(|ui| {
        if crate::widgets::pill_button(ui, tl!("Add to portfolio"), false).clicked() {
            add_file = true;
        }
    });
    ui.add_space(12.0);

    egui::Grid::new("portfolio-draft").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        let l = ui.label(tl!("Title"));
        ui.add(egui::TextEdit::singleline(&mut d.title).desired_width(260.0)).labelled_by(l.id);
        ui.end_row();

        ui.label(tl!("View"));
        egui::ComboBox::from_id_salt("portfolio-view")
            .selected_text(view_name(d.view))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut d.view, PortfolioView::Tile, "Tile");
                ui.selectable_value(&mut d.view, PortfolioView::List, "List");
                ui.selectable_value(&mut d.view, PortfolioView::Details, "Details");
                ui.selectable_value(&mut d.view, PortfolioView::Hidden, "Hidden");
            });
        ui.end_row();
    });

    // Details-view columns.
    if d.view == PortfolioView::Details {
        ui.label(tl!("Details-view columns"));
        ui.add_space(4.0);
        for i in 0..d.fields.len() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&d.fields[i].name).small());
                ui.label(
                    egui::RichText::new(&format!("← {}", d.fields[i].source))
                        .small()
                        .color(t.text_faint),
                );
                if ui.small_button("×").clicked() {
                    d.fields.remove(i);
                }
            });
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut d.new_field_name)
                    .hint_text(tl!("Column header"))
                    .desired_width(120.0),
            );
            egui::ComboBox::from_id_salt("pf-source")
                .selected_text(&d.new_field_source)
                .show_ui(ui, |ui| {
                    for s in ["Name", "Description", "Size", "Modified", "Created"] {
                        ui.selectable_value(&mut d.new_field_source, s.to_string(), s);
                    }
                });
            if ui.button(tl!("Add column")).clicked() && !d.new_field_name.trim().is_empty() {
                d.fields.push(CollectionField {
                    name: d.new_field_name.trim().to_string(),
                    source: d.new_field_source.clone(),
                });
                d.new_field_name.clear();
            }
        });
        ui.add_space(8.0);
    }

    let mut create = false;
    let mut clear = false;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if crate::widgets::pill_button(ui, tl!("Create portfolio"), true).clicked() {
            create = true;
        }
        ui.add_space(8.0);
        if crate::widgets::pill_button(ui, tl!("Clear portfolio"), false).clicked() {
            clear = true;
        }
        ui.add_space(8.0);
        if crate::widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
            // cancel handled by caller via close
        }
    });
    let (title, view, fields) = if let Some(d) = &app.portfolio_draft {
        (d.title.clone(), d.view, d.fields.clone())
    } else {
        (String::new(), PortfolioView::Tile, Vec::new())
    };
    (create, clear, add_file, title, view, fields)
}

fn view_name(v: PortfolioView) -> &'static str {
    match v {
        PortfolioView::Tile => "Tile",
        PortfolioView::List => "List",
        PortfolioView::Details => "Details",
        PortfolioView::Hidden => "Hidden",
    }
}
