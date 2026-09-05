# Multimodal, Media and Notebook Runtime

> **LOCKED:** media is first-class typed content. It is not smuggled through ad-hoc strings, screenshots or provider-specific message shapes.

## Canonical media model

`MediaEnvelope` carries content digest/reference, media type, MIME, dimensions/duration/page metadata, source provenance, extraction/transform lineage, size budget, trust label and optional text/structured derivative. Large bytes live in Artifact Store; events and tool results carry references.

## File/media read behavior

`fs.read` returns bounded typed text/media/notebook results. Images remain images. PDFs use deterministic text/structure extraction first and may escalate selected pages/regions to vision when text extraction is insufficient. Audio/video expose bounded metadata/transcript/frames according to model capability and task need. Notebook reads preserve cell identity/type/outputs; edits are revision-bound cell operations rather than whole-file string replacement where possible.

## Model capability metadata

Provider/model registration declares supported input/output modalities, context/media limits and tool-role media constraints. Prompt compilation projects only representations supported by the selected model. Provider adapters may transform placement/encoding but must preserve canonical `ModelEvent`/`ToolResult` semantics.

## Provider media normalization

Some transports cannot embed media directly inside a tool-result message. The adapter may split the canonical result into a compliant sequence while preserving call identity, provenance and ordering. This transformation is provider-local and never changes Core state contracts.

## Subagent continuation

Agent execution capsules declare allowed modalities and tools. Child agents may continue durably/background only when WorkGraph dependency analysis says the parent can proceed without their immediate result. Identity, lineage, private context refs, event offsets and result envelope survive restart.

## Import compatibility

Foreign instruction manifests, command packs, skill bundles, agent definitions and external-tool declarations may be imported through a compatibility adapter. Import is a migration into Modbit schemas, not execution of a foreign runtime. Imported executable material is discovered but untrusted and receives no authority beyond normal policy.

## Security

Media/document content is untrusted data. Embedded instructions cannot modify system policy or capability grants. Enforce decompression/page/frame/size/time limits; scan/archive metadata must not expose secrets; generated previews and extracted text retain provenance back to original bytes.

## Completion proof

Real tests must cover PNG/JPEG, text PDF, scanned PDF with bounded vision fallback, audio, video, notebook cells, oversized/decompression abuse, hostile embedded instructions, provider-specific media serialization and restart/retrieval by artifact digest.
