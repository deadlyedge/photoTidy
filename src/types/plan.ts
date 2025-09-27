export type ExecutionMode = "copy" | "move"

export interface PlanItem {
  fileHash: string
  fileSize: number
  originFileName: string
  originFullPath: string
  newFileName: string
  newPath: string
  isDuplicate: boolean
}

export interface PlanSummary {
  generatedAt: string
  totalEntries: number
  duplicateEntries: number
  uniqueEntries: number
  destinationBuckets: number
  totalBytes: number
  planJsonPath: string
  entries: PlanItem[]
}

export interface PlanProgressPayload {
  stage: "plan"
  processed: number
  total: number
  current?: string
}

export interface ExecutionSummary {
  mode: ExecutionMode
  dryRun: boolean
  totalEntries: number
  processedEntries: number
  succeeded: number
  failed: number
  duplicateEntries: number
}

export interface ExecutionProgressPayload {
  stage: "execute" | "undo"
  processed: number
  total: number
  current?: string
}

export interface UndoSummary {
  processedEntries: number
  restored: number
  missing: number
  failed: number
}