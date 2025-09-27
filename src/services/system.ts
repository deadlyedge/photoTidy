import { invoke } from "@tauri-apps/api/core"

import type { DiskStatus } from "../types/system"

export async function checkDiskSpace(): Promise<DiskStatus> {
  return invoke<DiskStatus>("check_disk_space")
}