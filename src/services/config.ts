import { invoke } from "@tauri-apps/api/core"

import type { AppConfig, RawConfigPayload } from "../types/config"

export const CONFIG_BOOTSTRAP_EVENT = "config://bootstrap"

export async function fetchBootstrapConfig(): Promise<AppConfig> {
  const payload = await invoke<RawConfigPayload>("bootstrap_paths")
  return normalizeConfig(payload)
}

export function normalizeConfig(payload: RawConfigPayload): AppConfig {
  return {
    schemaVersion: payload.schema_version,
    databasePath: payload.database_path,
    imageRoot: payload.image_root,
    imageRootDefaultName: payload.image_root_default_name,
    outputRoot: payload.output_root,
    outputRootName: payload.output_root_name,
    duplicatesDir: payload.duplicates_dir,
    duplicatesFolderName: payload.duplicates_folder_name,
    originInfoPath: payload.origin_info_json,
    targetPlanPath: payload.target_plan_json,
    imageExtensions: [...payload.image_exts].sort(),
    sampleImageRoot: payload.sample_image_root ?? undefined,
  }
}
