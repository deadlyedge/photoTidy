import { invoke } from "@tauri-apps/api/core"

import type { ScanSummary } from "../types/scan"

export const SCAN_PROGRESS_EVENT = "scan://progress"

export async function scanMedia(): Promise<ScanSummary> {
  return invoke<ScanSummary>("scan_media")
}