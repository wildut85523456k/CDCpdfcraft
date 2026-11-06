//! PDF Portfolio (ISO 32000-1 §7.11): the `/Collection` dictionary that turns a document's
//! embedded files into a portfolio with a tile/list/details view. The embedded files themselves
//! are attached via `attachments::add_attachment`; the `/Collection` dictionary in the catalog
//! tells the viewer to display them as a portfolio rather than plain attachments.

use pdfcraft_cos::{Dict, Document, Object, PdfString};

use crate::OrganizeError;
use crate::attachments::attachment_keys;

/// The portfolio's initial view: how Acrobat shows the embedded files in the side panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PortfolioView {
    /// Acrobat's default: a grid of thumbnails with file names beneath.
    #[default]
    Tile,
    /// A simple list of file names.
    List,
    /// A detailed list with column headers (from [`CollectionField`]s).
    Details,
    /// No file side panel — only the document's pages are shown.
    Hidden,
}

/// A column shown in the portfolio's Details view. Each maps a header label to a property
/// of the file specification the viewer reads from the embedded file's parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectionField {
    /// The column header text (e.g. "Name", "Size", "Modified").
    pub name: String,
    /// The source property: "Name" (file name, default), "Description" (`/Desc`),
    /// "Size" (`/Params/Size`), "Modified" (`/Params/ModDate`) or "Created"
    /// (`/Params/CreationDate`).
    pub source: String,
}

/// Turn the document into a PDF Portfolio by adding a `/Collection` dictionary to the catalog.
/// Pass `None` for `title` to clear the portfolio (the embedded files remain as plain attachments).
/// The viewer uses `view` for the initial layout and `fields` for the Details columns.
pub fn set_portfolio(
    doc: &mut Document,
    title: Option<&str>,
    view: PortfolioView,
    fields: &[CollectionField],
) -> Result<(), OrganizeError> {
    let root = doc.root().ok_or(OrganizeError::NoPageTree)?;

    if title.is_none() {
        // Clearing: remove /Collection from the catalog.
        doc.update_dict(root, |c| {
            c.remove(b"Collection");
        })?;
        return Ok(());
    }

    let mut collection = Dict::new();
    collection.set(b"Type".to_vec(), Object::name("Collection"));

    let view_name: &str = match view {
        PortfolioView::Tile => "Tile",
        PortfolioView::List => "List",
        PortfolioView::Details => "Details",
        PortfolioView::Hidden => "Hidden",
    };
    collection.set(b"View".to_vec(), Object::name(view_name));

    // Optional /Fields: the Details-view columns.
    if !fields.is_empty() {
        let field_objs: Vec<Object> = fields
            .iter()
            .map(|f| {
                let mut field = Dict::new();
                field.set(b"Type".to_vec(), Object::name("CollectionField"));
                field.set(b"Name".to_vec(), Object::String(PdfString::text(&f.name)));
                // Map the source name to a `/Subtype` value. ISO 32000-1 §12.3.5.
                let sub: &str = match f.source.to_ascii_lowercase().as_str() {
                    "name" => "Name",
                    "description" => "Description",
                    "size" => "Size",
                    "modified" => "Modified",
                    "created" => "Created",
                    _ => {
                        // A custom /Name-subtype field.
                        let mut custom = Dict::new();
                        custom.set(b"Type".to_vec(), Object::name("CollectionField"));
                        custom.set(b"Name".to_vec(), Object::String(PdfString::text(&f.name)));
                        custom.set(b"Subtype".to_vec(), Object::name("Name"));
                        field = custom;
                        return Object::Dict(field);
                    }
                };
                field.set(b"Subtype".to_vec(), Object::name(sub));
                Object::Dict(field)
            })
            .collect();
        collection.set(b"Fields".to_vec(), Object::Array(field_objs));
    }

    doc.update_dict(root, |c| {
        c.set(b"Collection".to_vec(), Object::Dict(collection));
    })?;
    Ok(())
}

/// The keys of the document's embedded files (portfolio members), in name-tree order.
pub fn portfolio_files(doc: &Document) -> Vec<String> {
    attachment_keys(doc)
}
