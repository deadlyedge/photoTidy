import { invoke } from "@tauri-apps/api/core"

import type {
  ExecutionMode,
  ExecutionSummary,
  PlanSummary,
  UndoSummary,
} from "../types/plan"

export const PLAN_PROGRESS_EVENT = "plan://progress"
export const EXECUTION_PROGRESS_EVENT = "execute://progress"

export function planTargets(): Promise<PlanSummary> {
  return invoke<PlanSummary>("plan_targets")
}

export function executePlan(mode: ExecutionMode, dryRun = false): Promise<ExecutionSummary> {
  return invoke<ExecutionSummary>("execute_plan", { mode, dryRun })
}

export function undoMoves(): Promise<UndoSummary> {
  return invoke<UndoSummary>("undo_moves")
}