// Compile-time check that the field names in types.ts match the JSON the Rust engine writes
// (crates/engine/contract/*.json, kept current by the engine test `contract_fixtures`).
// Nothing imports this file; `npm run check` type-checks it. A field renamed, added or
// removed on one side only shows up here as an error naming the field.
import type { EncodeSpec, MediaInfo, Settings } from "./types";
import spec from "../../crates/engine/contract/encode_spec.json";
import media from "../../crates/engine/contract/media_info.json";
import settings from "../../crates/engine/contract/settings.json";

/** The keys only in A, only in B; both `never` when the two key sets are equal. */
type Diff<A, B> = { onlyInRust: Exclude<keyof A, keyof B>; onlyInTs: Exclude<keyof B, keyof A> };
type Same<A, B> = Diff<A, B> extends { onlyInRust: never; onlyInTs: never } ? true : Diff<A, B>;

export const contract: [
  Same<typeof spec, EncodeSpec>,
  Same<typeof spec.adjust, NonNullable<EncodeSpec["adjust"]>>,
  Same<typeof spec.audio, NonNullable<EncodeSpec["audio"]>>,
  Same<typeof spec.layout, NonNullable<EncodeSpec["layout"]>>,
  Same<typeof media, MediaInfo>,
  Same<typeof media.video, MediaInfo["video"]>,
  Same<(typeof media.audio)[number], MediaInfo["audio"][number]>,
  Same<typeof settings, Settings>,
] = [true, true, true, true, true, true, true, true];
