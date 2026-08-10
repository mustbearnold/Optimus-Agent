//! Free-text recall over claims: SQLite FTS5, no new dependency.
//!
//! `Memory::recall` answers an exact `(subject, predicate)` lookup, so a caller
//! who does not already know how a fact was phrased cannot reach it. This module
//! is the reach half — and it is deliberately built so that reach cannot become
//! authority (ADR-0072).
//!
//! Two invariants hold everything together:
//!
//! 1. **The index narrows; it never authorizes.** A MATCH yields candidate
//!    `claim_id`s and nothing else. Every candidate is re-read from `claims` and
//!    put through the same scope, clearance, and allowed-use gates as exact
//!    recall, so a stale or hand-edited index still cannot surface a claim above
//!    the caller's clearance or one that has been forgotten.
//! 2. **A derived index must not outlive the erasure of its source.** Deletion
//!    happens in [`crate::redaction`], inside the same transaction that closes
//!    the claim.
//!
//! There is a third, quieter decision. Exact recall drops knowledge-closed rows
//! in SQL (`tx_to IS NULL OR tx_to > as_of_tx`); this module keeps them and
//! *labels* them [`ClaimStanding::Superseded`]. A search that silently omits the
//! answer someone remembers giving reads as amnesia, and a search that returns
//! it unmarked reads as truth. Neither is honest, so the staleness is carried on
//! the hit and sorted below anything still believed.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    purpose_allowed_use, row_to_claim, ClaimView, Memory, MemoryError, RecallPurpose, Result,
    WriteContext,
};

/// Rows the index may hand the authorization pass for one query.
///
/// The cap is applied in SQL, before clearance is known, which is the same
/// ordering `Memory::recall` uses — scope in SQL, clearance in Rust. The bound
/// it accepts is that an authorized match ranked below the 512th lexical
/// candidate *within one scope* is not returned. That is far past any realistic
/// per-project claim count and is the price of not loading a whole store into
/// memory to answer one query.
const CANDIDATE_CEILING: i64 = 512;

/// End of the representable RFC3339 range: "everything ever known".
const DISTANT_FUTURE: &str = "9999-12-31T23:59:59Z";

/// Create the claim text index, backfilling an existing store on first open.
pub(crate) fn ensure_text_index(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS claims_fts USING fts5(
            subject, predicate, object, claim_id UNINDEXED,
            tokenize = 'porter unicode61'
        );",
    )?;
    let indexed: i64 = conn.query_row("SELECT count(*) FROM claims_fts", [], |row| row.get(0))?;
    if indexed > 0 {
        return Ok(());
    }
    // Backfill skips forgotten and erased rows rather than reindexing and then
    // deleting them: a store that was erased before this index existed must not
    // get its text back when it is opened by a newer build.
    conn.execute(
        "INSERT INTO claims_fts(subject, predicate, object, claim_id)
         SELECT subject, predicate, object, id FROM claims
         WHERE tombstoned_at IS NULL AND erased = 0",
        [],
    )?;
    Ok(())
}

pub(crate) fn index_claim(
    conn: &Connection,
    id: Uuid,
    subject: &str,
    predicate: &str,
    object: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO claims_fts(subject, predicate, object, claim_id) VALUES (?1, ?2, ?3, ?4)",
        params![subject, predicate, object, id.to_string()],
    )?;
    Ok(())
}

pub(crate) fn unindex_claim(conn: &Connection, id: Uuid) -> Result<()> {
    unindex_claim_raw(conn, &id.to_string())
}

pub(crate) fn unindex_claim_raw(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM claims_fts WHERE claim_id = ?1", params![id])?;
    Ok(())
}

/// Build a conservative FTS5 MATCH expression from free text.
///
/// Everything that is not alphanumeric, `_`, or `-` is dropped, and each
/// surviving token is quoted and prefix-matched. FTS5 treats `*`, `:`, `^`, and
/// `NEAR` as syntax, so passing user text through unquoted turns a search box
/// into a query-language console — and a malformed expression into an error
/// rather than an empty result.
fn match_expression(raw: &str) -> String {
    // Repeated words are ANDed in FTS5, so "key key key" would otherwise build
    // `"key"* "key"* "key"*` — redundant terms that cost nothing semantically
    // but grow the expression (and, for pathological input, risk the tokenizer
    // limit). Collapse them while preserving first-seen order.
    let mut seen = std::collections::HashSet::new();
    raw.split_whitespace()
        .filter_map(|token| {
            let cleaned: String = token
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            // The porter/unicode61 tokenizer treats `_` and `-` as word
            // separators, so a token made only of them has no searchable
            // content. Emit nothing rather than a useless `"-"*` term — the
            // same "pure punctuation reduces to nothing" rule already applied
            // to `!`, `?`, etc.
            if !cleaned.chars().any(char::is_alphanumeric) {
                return None;
            }
            let term = format!("\"{cleaned}\"*");
            // FTS5's unicode61 tokenizer folds case, so `Key key KEY` are the
            // same term to the index. Dedup on the folded form rather than the
            // literal spelling so differently-cased repeats collapse too.
            let key = cleaned.to_ascii_lowercase();
            seen.insert(key).then_some(term)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Where a matched claim stands relative to the instants the query asked about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStanding {
    /// Still believed, and true across the asked-about instant.
    Current,
    /// Its validity has not begun yet.
    NotYetValid,
    /// Its validity ended before the asked-about instant.
    Expired,
    /// A later correction closed this knowledge version.
    Superseded,
}

impl ClaimStanding {
    /// Sort weight. A claim that is not believed now can never outrank one that
    /// is, however well it matches the words — that inversion is precisely the
    /// memory-pollution failure this ranking exists to refuse.
    fn weight(self) -> u8 {
        match self {
            ClaimStanding::Current => 0,
            ClaimStanding::NotYetValid => 1,
            ClaimStanding::Expired => 2,
            ClaimStanding::Superseded => 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRecallQuery {
    pub purpose: RecallPurpose,
    pub text: String,
    /// World time to judge validity against. Defaults to now, not to the end of
    /// time, so `Current` means "true today" without the caller saying so.
    pub as_of_valid: Option<String>,
    /// Knowledge time to read the store at. Defaults to the end of time so that
    /// superseded versions are returned and labelled rather than dropped.
    pub as_of_tx: Option<String>,
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRecallHit {
    /// The whole claim, which is also its provenance: `origin`, `trust`,
    /// `confidence`, `tx_from` (when it was learned), `supersedes`, and
    /// `in_conflict` all travel with it.
    pub claim: ClaimView,
    pub standing: ClaimStanding,
    /// Its retention deadline had already passed at the asked-about instant —
    /// it is owed a `apply_retention` sweep and should not be leaned on.
    pub retention_due: bool,
    /// Lexical match strength, higher is better (negated BM25).
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRecallPacket {
    /// Stable fence label: recalled content is DATA, not instruction.
    pub fence: String,
    pub purpose: RecallPurpose,
    pub hits: Vec<TextRecallHit>,
    pub citations: Vec<Uuid>,
    /// Nothing believed now matched — either the words are unknown, or every
    /// claim carrying them is stale. Both mean the same thing to a caller: do
    /// not answer from memory.
    pub abstained: bool,
    /// Authorized matches were dropped to honour `limit`. Counted after the
    /// clearance filter, so it never reports the existence of claims the caller
    /// may not see.
    pub truncated: bool,
}

impl Memory {
    /// Search claim text, returning each match with its provenance and standing.
    ///
    /// Data only. Like [`Memory::recall`] this refuses
    /// [`RecallPurpose::ActionAuthorize`]: memory describes the world, it does
    /// not grant permission to act on it.
    pub fn recall_text(
        &self,
        ctx: &WriteContext,
        query: TextRecallQuery,
    ) -> Result<TextRecallPacket> {
        if matches!(query.purpose, RecallPurpose::ActionAuthorize) {
            return Err(MemoryError::ActionAuthorizeUnsupported);
        }
        let limit = query.limit.max(1) as usize;
        let as_of_valid = query
            .as_of_valid
            .clone()
            .unwrap_or_else(|| self.clock.now());
        let as_of_tx = query
            .as_of_tx
            .clone()
            .unwrap_or_else(|| DISTANT_FUTURE.to_string());

        let expression = match_expression(&query.text);
        if expression.is_empty() {
            // Punctuation, or nothing. An empty MATCH is a syntax error in FTS5;
            // a search for nothing is not an error, it just finds nothing.
            return Ok(empty_packet(query.purpose));
        }

        // Scope, forgetting, and knowledge-start in SQL; clearance and
        // allowed-use in Rust, exactly as `recall` splits them.
        let mut statement = self.conn.prepare(
            "SELECT c.id, c.tenant, c.user_id, c.agent, c.project, c.subject, c.predicate,
                    c.object, c.valid_from, c.valid_to, c.tx_from, c.tx_to, c.confidence,
                    c.origin, c.trust, c.allowed_uses_json, c.supersedes, c.in_conflict,
                    c.sensitivity, c.retention_until, c.tombstoned_at, c.erased,
                    bm25(claims_fts) AS lexical
             FROM claims_fts
             JOIN claims c ON c.id = claims_fts.claim_id
             WHERE claims_fts MATCH ?1
               AND c.tenant = ?2 AND c.user_id = ?3 AND c.project = ?4
               AND c.tx_from <= ?5
               AND c.tombstoned_at IS NULL AND c.erased = 0
             ORDER BY lexical
             LIMIT ?6",
        )?;
        let rows = statement.query_map(
            params![
                expression,
                ctx.tenant,
                ctx.user,
                ctx.project,
                as_of_tx,
                CANDIDATE_CEILING
            ],
            |row| Ok((row_to_claim(row)?, row.get::<_, f64>(22)?)),
        )?;

        let wanted = purpose_allowed_use(query.purpose);
        let mut hits: Vec<TextRecallHit> = Vec::new();
        for row in rows {
            let (claim, lexical) = row?;
            if claim.sensitivity > ctx.max_sensitivity || !claim.allowed_uses.contains(&wanted) {
                continue;
            }
            let retention_due = claim
                .retention_until
                .as_deref()
                .is_some_and(|deadline| deadline <= as_of_valid.as_str());
            hits.push(TextRecallHit {
                standing: standing_of(&claim, &as_of_valid, &as_of_tx),
                claim,
                retention_due,
                score: -lexical,
            });
        }
        drop(statement);

        hits.sort_by(|a, b| {
            a.standing
                .weight()
                .cmp(&b.standing.weight())
                .then(b.score.total_cmp(&a.score))
                .then(a.claim.id.cmp(&b.claim.id))
        });
        let truncated = hits.len() > limit;
        hits.truncate(limit);

        let abstained = !hits
            .iter()
            .any(|hit| hit.standing == ClaimStanding::Current);
        Ok(TextRecallPacket {
            fence: "EVIDENCE_DATA_NOT_INSTRUCTION_NOT_CAPABILITY".into(),
            purpose: query.purpose,
            citations: hits.iter().map(|hit| hit.claim.id).collect(),
            hits,
            abstained,
            truncated,
        })
    }
}

fn empty_packet(purpose: RecallPurpose) -> TextRecallPacket {
    TextRecallPacket {
        fence: "EVIDENCE_DATA_NOT_INSTRUCTION_NOT_CAPABILITY".into(),
        purpose,
        hits: Vec::new(),
        citations: Vec::new(),
        abstained: true,
        truncated: false,
    }
}

/// Knowledge time is read before world time on purpose: a claim that has been
/// corrected is stale whatever its validity window says, and reporting it as
/// `Expired` would blame the world for what the store already knows.
fn standing_of(claim: &ClaimView, as_of_valid: &str, as_of_tx: &str) -> ClaimStanding {
    if claim
        .tx_to
        .as_deref()
        .is_some_and(|closed| closed <= as_of_tx)
    {
        return ClaimStanding::Superseded;
    }
    if claim.valid_from.as_str() > as_of_valid {
        return ClaimStanding::NotYetValid;
    }
    if claim
        .valid_to
        .as_deref()
        .is_some_and(|ends| ends <= as_of_valid)
    {
        return ClaimStanding::Expired;
    }
    ClaimStanding::Current
}

#[cfg(test)]
mod tests {
    use super::match_expression;

    #[test]
    fn every_token_is_quoted_and_prefix_matched() {
        assert_eq!(match_expression("deploy key"), "\"deploy\"* \"key\"*");
    }

    #[test]
    fn fts_syntax_in_user_text_is_stripped_rather_than_executed() {
        // Unquoted, each of these is an operator: a search box would become a
        // query console, and `subject:` would let a caller aim at a column.
        assert_eq!(match_expression("subject:secret"), "\"subjectsecret\"*");
        assert_eq!(match_expression("a NEAR/2 b"), "\"a\"* \"NEAR2\"* \"b\"*");
        assert_eq!(match_expression("\"unbalanced"), "\"unbalanced\"*");
        assert_eq!(match_expression("wild*"), "\"wild\"*");
    }

    #[test]
    fn a_query_of_pure_punctuation_reduces_to_nothing() {
        // Which the caller turns into an empty result, never an FTS5 error.
        assert!(match_expression("!!! ??? ...").is_empty());
        assert!(match_expression("   ").is_empty());
    }

    #[test]
    fn hyphen_and_underscore_only_tokens_reduce_to_nothing() {
        // `-` and `_` are FTS5 word separators, so a token made only of them
        // carries no searchable content. Emit nothing rather than a useless
        // `"-"*` term, matching the pure-punctuation invariant above.
        assert!(match_expression("- - _").is_empty());
        assert!(match_expression("_").is_empty());
        assert_eq!(match_expression("deploy - key"), "\"deploy\"* \"key\"*");
    }

    #[test]
    fn hyphens_and_underscores_inside_words_survive() {
        // Real hyphenated/underscored words keep their joining characters.
        assert_eq!(match_expression("well-being"), "\"well-being\"*");
        assert_eq!(match_expression("snake_case"), "\"snake_case\"*");
    }

    #[test]
    fn non_ascii_words_survive() {
        assert_eq!(match_expression("café Zürich"), "\"café\"* \"Zürich\"*");
    }

    #[test]
    fn repeated_words_are_collapsed_not_repeated() {
        // "key key key" must not build `"key"* "key"* "key"*`: the extra ANDed
        // terms are redundant and only inflate the FTS5 expression.
        assert_eq!(match_expression("key key key"), "\"key\"*");
        assert_eq!(
            match_expression("deploy key deploy"),
            "\"deploy\"* \"key\"*"
        );
    }

    #[test]
    fn repeated_words_collapse_regardless_of_case() {
        // unicode61 folds case, so `Key key KEY` are one term to the index.
        // Dedup must use the folded form, not the literal spelling, or the
        // same word spelled differently would produce redundant ANDed terms.
        assert_eq!(match_expression("Key key KEY"), "\"Key\"*");
        assert_eq!(
            match_expression("Deploy key deploy KEY"),
            "\"Deploy\"* \"key\"*"
        );
    }
}
