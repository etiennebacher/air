//! The hand-written Pratt parser backend
//!
//! This backend must produce byte-identical `(events, trivia)` streams to the
//! tree-sitter backend for the whole corpus before it becomes the default
//! (see `tests/differential.rs`). That means it reproduces the *converted*
//! tree shape the tree-sitter walker synthesizes (expression lists, argument
//! holes, name clauses, ...), not tree-sitter's raw shape, and it fails the
//! way the tree-sitter backend fails: any error anywhere collapses the file
//! into a single bogus expression.
//!
//! Grammar functions return `Option<CompletedMarker>`: `None` means a parse
//! error was recorded and the whole file will be reported as a failure, so
//! callers just abandon their markers and propagate.

mod atoms;
mod calls;
mod control;
mod expressions;
mod functions;

use air_r_syntax::RSyntaxKind;
use biome_parser::Parser;
use biome_parser::event::Event;
use biome_parser::prelude::CompletedMarker;
use biome_parser::prelude::Trivia;

use crate::ParseError;
use crate::parser::RParser;

pub(crate) fn parse_text(
    text: &str,
) -> (Vec<Event<RSyntaxKind>>, Vec<Trivia>, Option<ParseError>) {
    let mut p = RParser::new(text);

    let ok = parse_root(&mut p).is_some() && !p.failed();
    if !ok {
        return crate::parse::parse_failure(text);
    }

    let (events, trivia) = p.finish();
    (events, trivia, None)
}

fn parse_root(p: &mut RParser) -> Option<CompletedMarker> {
    let m = p.start();

    // NOTE: the tree-sitter backend panics on a leading BOM; here it fills
    // the root's optional bom slot instead
    if p.at(RSyntaxKind::UNICODE_BOM) {
        p.bump(RSyntaxKind::UNICODE_BOM);
    }

    let Some(_) = parse_expression_list(p, RSyntaxKind::EOF) else {
        m.abandon(p);
        return None;
    };

    // The tree sink adds the EOF token (with the remaining trivia) itself
    Some(m.complete(p, RSyntaxKind::R_ROOT))
}

/// The statement list of the program or of a `{}` block, running until
/// `end` (`EOF` or `R_CURLY`)
///
/// This is the only place where semicolons are legal, so each iteration
/// blesses the separators preceding the upcoming token (see
/// `token_source.rs`).
pub(crate) fn parse_expression_list(
    p: &mut RParser,
    end: RSyntaxKind,
) -> Option<CompletedMarker> {
    let m = p.start();

    loop {
        p.bless_semicolons();

        if p.at(end) || p.at(RSyntaxKind::EOF) {
            break;
        }

        let Some(_) = expressions::parse_expression(p, 0) else {
            m.abandon(p);
            return None;
        };
    }

    Some(m.complete(p, RSyntaxKind::R_EXPRESSION_LIST))
}

/// Record an error diagnostic at the current token. Message contents don't
/// matter in the parity phase (any error collapses into the fixed whole-file
/// failure message), but presence does.
fn expected(p: &mut RParser, what: &str) {
    let range = p.cur_range();
    let diagnostic = p.err_builder(format!("expected {what}"), range);
    p.error(diagnostic);
}
