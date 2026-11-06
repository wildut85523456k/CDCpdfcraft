//! PDF Portfolio: the catalog's `/Collection` dictionary (ISO 32000-1 §7.11) that presents the
//! document's embedded files in a side panel, with an initial view and Details-view columns.

use std::sync::Arc;

use pdfcraft_cos::{Dict, Document, Object, PdfString, SaveOptions, write_full};
use pdfcraft_organize::{
    CollectionField, PortfolioView, add_attachment, attachment_keys, portfolio_files, set_portfolio,
};

/// One page, with a catalog and page tree, but no `/Names` yet.
fn one_page_pdf() -> Vec<u8> {
    let objs: [&[u8]; 3] = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>",
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn catalog_of(doc: &Document) -> Dict {
    let root = doc.root().unwrap();
    doc.dict(&Object::Ref(root)).unwrap()
}

/// The catalog's `/Collection` dictionary, resolved (an indirect dictionary counts).
fn collection_of(doc: &Document) -> Option<Dict> {
    catalog_of(doc).get(b"Collection").and_then(|c| doc.dict(c))
}

/// A resolved object's `/Name` value, as text.
fn name_of(doc: &Document, o: &Object) -> Option<String> {
    doc.resolve(o).as_name().map(|n| String::from_utf8_lossy(n).into_owned())
}

#[test]
fn a_portfolio_adds_a_collection_dictionary_to_the_catalog() {
    let mut doc = Document::open(Arc::new(one_page_pdf())).unwrap();
    assert!(collection_of(&doc).is_none(), "no portfolio to begin with");

    set_portfolio(&mut doc, Some("Signed set"), PortfolioView::Tile, &[]).unwrap();
    let c = collection_of(&doc).expect("the catalog has a /Collection dictionary");
    assert_eq!(name_of(&doc, &Object::name("Collection")), Some("Collection".into()));
    assert_eq!(c.get(b"Type").and_then(|t| name_of(&doc, t)), Some("Collection".into()));
    assert_eq!(c.get(b"View").and_then(|v| name_of(&doc, v)), Some("Tile".into()));
}

#[test]
fn every_view_name_is_written_as_its_own_name_object() {
    for (view, expected) in [
        (PortfolioView::Tile, "Tile"),
        (PortfolioView::List, "List"),
        (PortfolioView::Details, "Details"),
        (PortfolioView::Hidden, "Hidden"),
    ] {
        let mut doc = Document::open(Arc::new(one_page_pdf())).unwrap();
        set_portfolio(&mut doc, Some("P"), view, &[]).unwrap();
        let c = collection_of(&doc).expect("a /Collection dictionary");
        assert_eq!(c.get(b"View").and_then(|v| name_of(&doc, v)), Some(expected.into()));
    }
}

#[test]
fn details_view_columns_are_written_as_collection_fields() {
    let fields = [
        CollectionField { name: "File".into(), source: "Name".into() },
        CollectionField { name: "Size".into(), source: "Size".into() },
        CollectionField { name: "Modified".into(), source: "Modified".into() },
        CollectionField { name: "Created".into(), source: "Created".into() },
        CollectionField { name: "Notes".into(), source: "Description".into() },
    ];
    let mut doc = Document::open(Arc::new(one_page_pdf())).unwrap();
    set_portfolio(&mut doc, Some("Set"), PortfolioView::Details, &fields).unwrap();
    let c = collection_of(&doc).expect("a /Collection dictionary");
    let list = c.get(b"Fields").and_then(|f| doc.resolve(f).as_array().cloned()).expect("/Collection /Fields is an array");
    assert_eq!(list.len(), fields.len());
    for (i, f) in fields.iter().enumerate() {
        let fd = doc.resolve(&list[i]).as_dict().cloned().expect("each field is a dictionary");
        assert_eq!(fd.get(b"Type").and_then(|t| name_of(&doc, t)), Some("CollectionField".into()));
        let name = fd.get(b"Name").and_then(|n| doc.resolve(n).as_string().map(|s| s.to_text()));
        assert_eq!(name.as_deref(), Some(f.name.as_str()));
        let sub = fd.get(b"Subtype").and_then(|s| name_of(&doc, s));
        assert_eq!(sub.as_deref(), Some(f.source.as_str()), "field {i} keeps its source");
    }
}

#[test]
fn an_unknown_source_still_gets_a_field_but_keeps_the_name_subtype() {
    let fields = [CollectionField { name: "Department".into(), source: "Department".into() }];
    let mut doc = Document::open(Arc::new(one_page_pdf())).unwrap();
    set_portfolio(&mut doc, Some("Set"), PortfolioView::Details, &fields).unwrap();
    let c = collection_of(&doc).expect("a /Collection dictionary");
    let list = c.get(b"Fields").and_then(|f| doc.resolve(f).as_array().cloned()).expect("/Fields is an array");
    let fd = doc.resolve(&list[0]).as_dict().cloned().expect("the field is a dictionary");
    assert_eq!(fd.get(b"Subtype").and_then(|s| name_of(&doc, s)), Some("Name".into()));
}

#[test]
fn clearing_the_portfolio_removes_the_collection_but_keeps_the_files() {
    let mut doc = Document::open(Arc::new(one_page_pdf())).unwrap();
    add_attachment(&mut doc, "contract.pdf", b"%PDF-1.7 signed", Some("Signed"), Some("application/pdf"), None).unwrap();
    add_attachment(&mut doc, "terms.pdf", b"%PDF-1.7 terms", None, Some("application/pdf"), None).unwrap();
    set_portfolio(&mut doc, Some("Deal"), PortfolioView::Tile, &[]).unwrap();
    assert!(collection_of(&doc).is_some());
    assert_eq!(portfolio_files(&doc), attachment_keys(&doc));

    set_portfolio(&mut doc, None, PortfolioView::Tile, &[]).unwrap();
    assert!(collection_of(&doc).is_none(), "the /Collection is gone");
    assert_eq!(attachment_keys(&doc).len(), 2, "the embedded files stay");
    assert_eq!(portfolio_files(&doc).len(), 2, "and are still listed");
}

#[test]
fn a_portfolio_with_no_files_is_still_valid() {
    let mut doc = Document::open(Arc::new(one_page_pdf())).unwrap();
    set_portfolio(&mut doc, Some("Empty"), PortfolioView::List, &[]).unwrap();
    assert!(collection_of(&doc).is_some());
    assert!(portfolio_files(&doc).is_empty());
}

#[test]
fn a_portfolio_survives_a_save_and_reopen() {
    let mut doc = Document::open(Arc::new(one_page_pdf())).unwrap();
    add_attachment(&mut doc, "a.pdf", b"%PDF-1.7 a", None, Some("application/pdf"), None).unwrap();
    set_portfolio(&mut doc, Some("Set"), PortfolioView::Details, &[CollectionField { name: "File".into(), source: "Name".into() }]).unwrap();
    let bytes = write_full(&doc, &SaveOptions::default()).unwrap();
    let reopened = Document::open(Arc::new(bytes)).unwrap();
    let c = collection_of(&reopened).expect("the /Collection survives a save");
    assert_eq!(c.get(b"View").and_then(|v| name_of(&reopened, v)), Some("Details".into()));
    assert_eq!(portfolio_files(&reopened), attachment_keys(&reopened));
    let _ = PdfString::literal(b"x");
}
