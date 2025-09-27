export interface ScanSummary {
  totalFiles: number
  hashedFiles: number
  skippedFiles: number
  duplicateFiles: number
}

export type ScanStage = "scan" | "diff" | "hash"

export interface ScanProgressPayload {
  stage: ScanStage
  processed: number
  total: number
  current?: string
}
