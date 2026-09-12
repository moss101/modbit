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

/// One page of a document handed over as an image because the page has no
/// extractable text (docs/25 "may escalate selected pages/regions to
/// vision when text extraction is insufficient"; REQ-EV-0185). Lossy and
/// untrusted by construction: it is the embedded scan, not a rendering.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageImage {
    /// 1-based page.
    pub page: u32,
    /// Egress copy of the page image (metadata stripped), by digest.
    pub egress_ref: String,
    /// MIME of the egress copy.
    pub mime: String,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
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
    /// Pages handed over as images because their text could not be
    /// extracted (scanned pages): bounded by the page budget; lossy and
    /// untrusted (REQ-EV-0185). Empty for every other read.
    #[serde(default)]
    pub page_images: Vec<PageImage>,
    /// The input modality a model needs to take this media as more than a
    /// digest: `image` | `document` | `audio` | `video` | `text`
    /// (REQ-EV-0184: an unsupported modality is said, never dropped).
    #[serde(default)]
    pub input_modality: String,
    /// The region an image was cropped to (`x, y, width, height` in the
    /// original), when the read asked for one (REQ-EV-0223).
    #[serde(default)]
    pub region: Option<(u32, u32, u32, u32)>,
}
