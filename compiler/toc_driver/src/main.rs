//! Dummy bin for running the new scanner and parser

use std::collections::HashMap;
use std::ops::Range;
use std::{env, fs, io, sync::Arc};

use toc_hir::db;
use toc_vfs::FileDb;

fn load_contents(path: &str) -> io::Result<String> {
    let contents = fs::read(path)?;
    let contents = String::from_utf8_lossy(&contents).to_string();
    Ok(contents)
}

fn main() {
    let path: String = env::args().nth(1).expect("Missing path to source file");
    let contents = load_contents(&path).expect("Unable to load file");
    let file_db = FileDb::new();

    // Add the root path to the file db
    let root_file = file_db.add_file(&path, &contents);
    let hir_db = db::HirBuilder::new();

    // Parse root CST
    let parsed = {
        let info = file_db.get_file(root_file);
        let parsed = toc_parser::parse(Some(root_file), &info.source);
        let dependencies = toc_driver::gather_dependencies(Some(root_file), parsed.syntax());
        // TODO: Gather dependencies from root CST, and parse them

        println!("Parsed output: {}", parsed.dump_tree());
        println!("Dependencies: {:#?}", dependencies);

        parsed
    };

    // TODO: Deal with include globs

    let (validate_res, hir_res) = {
        let validate_res = toc_validate::validate_ast(Some(root_file), parsed.syntax());
        let hir_res = toc_hir_lowering::lower_ast(hir_db.clone(), Some(root_file), parsed.syntax());

        (validate_res, hir_res)
    };

    let hir_db = hir_db.finish();
    let root_unit = hir_db.get_unit(hir_res.id);
    println!("{:#?}", root_unit);

    // TODO: resolve imports between units

    let analyze_res = toc_analysis::analyze_unit(hir_db.clone(), hir_res.id);

    let mut msgs = parsed
        .messages()
        .iter()
        .chain(validate_res.messages().iter())
        .chain(hir_res.messages().iter())
        .chain(analyze_res.messages().iter())
        .collect::<Vec<_>>();

    // Sort by start order
    msgs.sort_by_key(|msg| msg.span().range.start());

    let mut has_errors = false;

    let span_mapper = SpanMapper::new(&file_db);

    for msg in msgs {
        has_errors |= matches!(msg.kind(), toc_reporting::AnnotateKind::Error);
        let snippet = span_mapper.message_into_snippet(msg);
        let display_list = annotate_snippets::display_list::DisplayList::from(snippet);

        println!("{}", display_list);
    }

    std::process::exit(if has_errors { -1 } else { 0 });
}

struct SpanMapper {
    files: HashMap<toc_span::FileId, (Arc<toc_vfs::FileInfo>, Vec<Range<usize>>)>,
}

impl SpanMapper {
    fn new(file_db: &toc_vfs::FileDb) -> Self {
        let mut files = HashMap::new();

        for file in file_db.files() {
            let info = file_db.get_file(file);
            let line_ranges = Self::build_line_ranges(&info.source);

            files.insert(file, (info, line_ranges));
        }

        Self { files }
    }

    fn build_line_ranges(source: &str) -> Vec<Range<usize>> {
        let mut line_ranges = vec![];
        let mut line_start = 0;
        let line_ends = source.char_indices().filter(|(_, c)| matches!(c, '\n'));

        for (at_newline, _) in line_ends {
            let line_end = at_newline + 1;
            line_ranges.push(line_start..line_end);
            line_start = line_end;
        }

        // Use a line span covering the rest of the file
        line_ranges.push(line_start..source.len());

        line_ranges
    }

    fn map_byte_index(
        &self,
        file: Option<toc_span::FileId>,
        byte_idx: usize,
    ) -> Option<(usize, Range<usize>)> {
        self.files.get(file.as_ref()?).and_then(|(_, line_ranges)| {
            line_ranges
                .iter()
                .enumerate()
                .find(|(_line, range)| range.contains(&byte_idx))
                .map(|(line, range)| (line, range.clone()))
        })
    }

    fn message_into_snippet<'a>(
        &'a self,
        msg: &'a toc_reporting::ReportMessage,
    ) -> annotate_snippets::snippet::Snippet<'a> {
        use annotate_snippets::{display_list::FormatOptions, snippet::*};

        // Build a set of common snippets for consecutive annotations
        struct FileSpan<'a> {
            span: toc_span::Span,
            source_range: Range<usize>,
            line_range: Range<usize>,
            source_slice: &'a str,
        }

        let mut file_spans = vec![FileSpan {
            span: msg.span(),
            source_range: 0..0,
            line_range: 0..0,
            source_slice: "",
        }];

        // Merge spans together
        for annotation in msg.annotations() {
            let span = annotation.span();
            let FileSpan {
                span: last_span, ..
            } = file_spans.last_mut().unwrap();

            if span.file == last_span.file {
                // Merge spans
                last_span.range = last_span.range.cover(span.range);
            } else {
                // Add a new span
                file_spans.push(FileSpan {
                    span,
                    source_range: 0..0,
                    line_range: 0..0,
                    source_slice: "",
                });
            }
        }

        // Get line spans
        for file_span in file_spans.iter_mut() {
            let (start, end) = (
                u32::from(file_span.span.range.start()),
                u32::from(file_span.span.range.end()),
            );
            let (start_line, start_range) = self
                .map_byte_index(file_span.span.file, start as usize)
                .unwrap();
            let (end_line, end_range) = self
                .map_byte_index(file_span.span.file, end as usize - 1)
                .unwrap();

            let source = &self
                .files
                .get(&file_span.span.file.unwrap())
                .unwrap()
                .0
                .source;
            file_span.source_range = start_range.start..end_range.end;
            file_span.line_range = start_line..end_line;
            file_span.source_slice = &source[start_range.start..end_range.end];
        }

        let file_spans = file_spans;

        // Build snippet slices & footers
        fn annotate_kind_to_type(kind: toc_reporting::AnnotateKind) -> AnnotationType {
            match kind {
                toc_reporting::AnnotateKind::Note => AnnotationType::Note,
                toc_reporting::AnnotateKind::Info => AnnotationType::Info,
                toc_reporting::AnnotateKind::Warning => AnnotationType::Warning,
                toc_reporting::AnnotateKind::Error => AnnotationType::Error,
            }
        }

        fn span_into_annotation<'a, 'b>(
            annotate_type: AnnotationType,
            span: toc_span::Span,
            label: &'a str,
            file_span: &'b FileSpan,
        ) -> SourceAnnotation<'a> {
            let FileSpan { source_range, .. } = file_span;
            let (start, end) = (u32::from(span.range.start()), u32::from(span.range.end()));

            let range_base = source_range.start;
            let real_slice = (start as usize - range_base)..(end as usize - range_base);

            // Get the real start & end, in characters
            // `annotate-snippets` requires that the range bounds are in characters, not byte indices
            let real_start = file_span.source_slice[0..real_slice.start].chars().count();
            let real_end = real_start + file_span.source_slice[real_slice].chars().count();

            SourceAnnotation {
                annotation_type: annotate_type,
                label,
                range: (real_start, real_end),
            }
        }

        let create_snippet = |file_span: &FileSpan| {
            let FileSpan {
                span,
                source_range,
                line_range,
                ..
            } = file_span;

            let file = span.file.unwrap();
            let source = &self.files.get(&file).unwrap().0.source;
            let slice_text = &source[source_range.clone()];
            let can_fold = (line_range.end - line_range.start) > 10;

            Slice {
                source: slice_text,
                line_start: line_range.start + 1,
                origin: Some(&self.files.get(&file).unwrap().0.path),
                annotations: vec![],
                fold: can_fold,
            }
        };

        let mut slices = vec![];
        let mut footer = vec![];
        let mut report_spans = file_spans.iter().peekable();

        // Insert the first slice
        let mut current_file = msg.span().file;

        {
            let annotation = span_into_annotation(
                annotate_kind_to_type(msg.kind()),
                msg.span(),
                "", // part of the larger message
                report_spans.peek().unwrap(),
            );

            let mut slice = create_snippet(report_spans.peek().unwrap());
            slice.annotations.push(annotation);
            slices.push(slice);
        }

        for annotate in msg.annotations() {
            let annotation = span_into_annotation(
                annotate_kind_to_type(annotate.kind()),
                annotate.span(),
                annotate.message(),
                report_spans.peek().unwrap(),
            );

            if current_file != annotate.span().file {
                current_file = annotate.span().file;

                let mut slice = create_snippet(report_spans.peek().unwrap());
                slice.annotations.push(annotation);
                slices.push(slice);
            } else {
                let slice = slices.last_mut().unwrap();
                slice.annotations.push(annotation);
            }
        }

        for annotate in msg.footer() {
            footer.push(Annotation {
                annotation_type: annotate_kind_to_type(annotate.kind()),
                id: None,
                label: Some(annotate.message()),
            });
        }

        let snippet = Snippet {
            title: Some(Annotation {
                label: Some(msg.message()),
                id: None,
                annotation_type: annotate_kind_to_type(msg.kind()),
            }),
            footer,
            slices,
            opt: FormatOptions {
                color: true,
                ..Default::default()
            },
        };

        snippet
    }
}
