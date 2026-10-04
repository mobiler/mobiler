# ADR-0049: A photo the pipeline produces is a short-lived handle — on Android and iOS pruned by a later `pick_photo_with` / `capture_photo_with` call once more than 24 hours old (the OS may clear its cache sooner), the 32 most recent valid on the web — and an app uploads or copies it rather than keeping it in saved state

Status:        Accepted
Date decided:  2026-10-03
Deciding PRs:  #283
Supersedes:    none
Code anchor:   demos/barbershop/Android/app/src/main/java/dev/mobiler/barbershop/PhotoPipeline.kt (`prune`, `KEEP_MS`), demos/barbershop/iOS/Sources/PhotoPipeline.swift (`prune`, `keep`, `photosDir`), their template copies under mobiler/templates/, mobiler-web/src/lib.rs (`PRODUCED_KEPT`, `remember_produced`, `produced_url`), mobiler-core/src/photo.rs (`Photo` docs)
Conformance:   mobiler-web/src/lib.rs::produced_urls_keep_the_32_most_recent

## 1. Context (The Problem)

ADR-0045 made `pick_photo_with` / `capture_photo_with` deliver a new file the shell writes: a `file://` in the app's
cache on Android and iOS, a `blob:` URL on the web. Its §5 listed as a Negative that "the produced files in the cache
are not pruned by the framework". Every photo an app took stayed on disk until the OS evicted the cache, and on the web
every `blob:` URL stayed in memory for the life of the page. A form that lets the user retake a photo, or a long web
session, accumulates them. Nothing told an app how long a handle stays valid.

## 2. Hypothesis

A bounded, documented lifetime keeps the disk and memory used by produced photos small, without breaking an app that
uses a photo the way photos are used: shown, then uploaded within the session. The bounds:

- **Android and iOS:** at the start of every pipeline call, delete the pipeline's earlier output that is more than
  24 hours old.
  - Only the pipeline's own folder is pruned: Android `cacheDir/photos/`, iOS `tmp/photos/` (a pass-through is copied
    there too, with a fresh date).
  - The current call's source and output are never touched.
- **Web:** keep the 32 most recent `blob:` URLs the pipeline produced, and revoke older ones. A page can't age files
  out the way a cache folder can, and no `blob:` URL survives a reload anyway.
- **Documentation:** `Photo`'s docs, `pick_photo_with`'s docs and the README say the handle is short-lived. An app
  uploads it, or copies what it needs, soon after, and doesn't keep it in `cx.save` state.

### 2.1. Refutation Conditions

- **Condition 1: the web keeps exactly the 32 most recent produced URLs.**
  - **Validation Metric:** `produced_urls_keep_the_32_most_recent`.
- **Condition 2 (runtime):** on Android, a pipeline output older than 24 hours is deleted by the next call, while
  newer ones and the new output stay. Checked on an API 36 emulator in PR #283: a 56-hour-old file was removed, and a
  fresh file and the previous output were kept.

## 3. Considered Options & Rationale for Refutation

- **Option A: never prune (ADR-0045's state)** `[recorded: ADR-0045 §5]`
  Rejected. The disk and memory grow without bound over an app's life.
- **Option B: an explicit `release(handle)` API** `[reconstructed]`
  Rejected for now. Every app would need to call it correctly, it is an ABI addition, and a forgotten call leaks as
  before.
- **Option C: a size-based cache (LRU by bytes)** `[reconstructed]`
  Rejected. An app could not know when its handle expires: a burst of large photos could evict one it is about to
  upload. A time bound is predictable.
- **Option D: 24 hours on native, the 32 most recent on web** `[recorded: maintainer, 2026-10-03, quoted here]`
  Proposed as "each photo call first deletes pipeline output older than 24 hours" and "the web keeps the 32 most
  recent photos the pipeline produced and releases older ones". The maintainer's reply: "yes please". Chosen. 24 hours covers any realistic show-then-upload flow, including an app sent to the background. 32 URLs cover
  a multi-photo form, and a page's memory use stays bounded.

## 4. Decision & Rationale for Corroboration

Option D.
- **Android** prunes `cacheDir/photos` by `lastModified`; **iOS** prunes `tmp/photos` by modification date. A
  pass-through copy on iOS (an APFS clone keeps the source's date) is stamped with the current time, so the next call
  can't prune it at once.
- **The web** records each produced URL (pass-through or re-encoded) and revokes the oldest beyond 32.
- **ADR-0045 is unchanged.** It is still the photo pipeline contract. This ADR retires two of its §5 Negatives: the
  unpruned cache (replaced by these bounds), and the black background of a transparent image written as JPEG, which
  is now drawn on white (a bug fix in the same PR).

**Mutation proof:**
- Keeping 33 URLs instead of 32 failed `produced_urls_keep_the_32_most_recent`.
- Reverting restored green.
- The native pruning has no unit harness in the shells. Condition 2 checks it at runtime on Android. On iOS, the
  pruning and the pass-through date stamp are checked by review only, pending a device check.

## 5. Consequences (Positive and Negative Predictions)

- **Positive:** the photos an app produces no longer pile up on disk or in a page's memory.
- **Positive:** an app knows how long a handle lasts, from the docs.
- **Negative:** a handle kept in saved state and used more than 24 hours later (native), or after 32 newer photos
  (web), is gone. An upload of it fails, and an image of it shows nothing. The docs say not to do this.
- **Negative:** the native pruning runs only when the pipeline runs: an app that stops taking photos keeps its last
  day's files until the OS evicts the cache.
- **Negative:** the bounds are fixed, not per-call options.
