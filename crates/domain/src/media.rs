//! Canonical media model (docs/25 "Canonical media model"): media is typed
//! content with provenance, budgets and lineage; large bytes live in the
//! artifact/object store and events and tool results carry references.

use serde::{Deserialize, Serialize};

/// Media type family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    /// Text.
    Text,
    /// Raster image.
    Image,
    /// Document (PDF).
    Document,
    /// Audio.
    Audio,
    /// Video.
    Video,
    /// Notebook.
    Notebook,
    /// Unknown binary.
    Binary,
}

/// Trust label of derived content (docs/25 "Security").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrustLabel {
    /// Bytes read from the user-approved workspace: data, never instructions.
    UntrustedWorkspaceContent,
    /// Text derived by a lossy transform (OCR/vision): untrusted and lossy.
    UntrustedLossyDerivative,
}

/// Where the media came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaProvenance {
    /// Root-relative path (or URL/source label).
    pub source: String,
    /// Workspace revision at read time.
    pub workspace_revision: Option<u64>,
    /// sha256 of the original bytes.
    pub original_digest: String,
    /// Task that read it.
    pub task_id: Option<crate::TaskId>,
}

/// One transform in the lineage (original → derivative).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaTransform {
    /// `exif_strip`, `pdf_text_extract`, `truncate`, `metadata_only`.
    pub name: String,
    /// Input digest.
    pub input_digest: String,
    /// Output digest (object hash), when bytes were produced.
    pub output_digest: Option<String>,
    /// Notes (pages covered, bytes dropped).
    pub detail: String,
}

/// Budget applied to the read (docs/25 "Security" limits).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaBudget {
    /// Max original bytes.
    pub max_bytes: u64,
    /// Max decoded pixels.
    pub max_pixels: u64,
    /// Max pages extracted.
    pub max_pages: u32,
    /// Max derivative text bytes returned inline.
    pub max_text_bytes: u64,
}

/// docs/25 `MediaEnvelope`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaEnvelope {
    /// Kind.
    pub kind: MediaKind,
    /// MIME type.
    pub mime: String,
    /// Object hash of the original bytes.
    pub content_ref: String,
    /// Original size in bytes.
    pub byte_length: u64,
    /// Object hash of the egress copy (metadata stripped), when produced.
    pub egress_ref: Option<String>,
    /// Width in pixels.
    pub width: Option<u32>,
    /// Height in pixels.
    pub height: Option<u32>,
    /// Page count.
    pub pages: Option<u32>,
    /// Pages covered by the text derivative (1-based, inclusive ranges).
    pub pages_covered: Vec<(u32, u32)>,
    /// Duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// Provenance.
    pub provenance: MediaProvenance,
    /// Lineage.
    pub lineage: Vec<MediaTransform>,
    /// Budget applied.
    pub budget: MediaBudget,
    /// Trust label of `text_derivative`.
    pub trust: TrustLabel,
    /// Bounded text derivative (extracted text, metadata summary).
    pub text_derivative: Option<String>,
    /// Whether the derivative was truncated.
    pub truncated: bool,
    /// Metadata dropped from the egress copy (tag names only, never values).
    pub metadata_stripped: Vec<String>,
}
