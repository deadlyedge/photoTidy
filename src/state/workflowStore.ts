import { create } from "zustand"

import { executePlan, planTargets, undoMoves } from "../services/plan"
import { scanMedia } from "../services/scan"
import type {
  ExecutionMode,
  ExecutionProgressPayload,
  ExecutionSummary,
  PlanProgressPayload,
  PlanSummary,
  UndoSummary,
} from "../types/plan"
import type { ScanProgressPayload, ScanSummary } from "../types/scan"

export type StageStatus = "idle" | "running" | "success" | "error"

interface OperationState<P, S> {
  status: StageStatus
  progress: P | null
  summary: S | null
  error?: string
}

interface WorkflowState {
  scan: OperationState<ScanProgressPayload, ScanSummary>
  plan: OperationState<PlanProgressPayload, PlanSummary>
  execution: OperationState<ExecutionProgressPayload, ExecutionSummary>
  undo: OperationState<ExecutionProgressPayload, UndoSummary>
  setScanProgress: (payload: ScanProgressPayload) => void
  setPlanProgress: (payload: PlanProgressPayload) => void
  setExecutionProgress: (payload: ExecutionProgressPayload) => void
  runScan: () => Promise<void>
  generatePlan: () => Promise<void>
  runExecution: (mode: ExecutionMode, dryRun: boolean) => Promise<void>
  runUndo: () => Promise<void>
  resetAfterConfig: () => void
}

function initialOperationState<P, S>(): OperationState<P, S> {
  return {
    status: "idle",
    progress: null,
    summary: null,
    error: undefined,
  }
}

function runningOperationState<P, S>(): OperationState<P, S> {
  return {
    status: "running",
    progress: null,
    summary: null,
    error: undefined,
  }
}

export const useWorkflowStore = create<WorkflowState>((set, get) => ({
  scan: initialOperationState<ScanProgressPayload, ScanSummary>(),
  plan: initialOperationState<PlanProgressPayload, PlanSummary>(),
  execution: initialOperationState<ExecutionProgressPayload, ExecutionSummary>(),
  undo: initialOperationState<ExecutionProgressPayload, UndoSummary>(),

  setScanProgress(payload) {
    set((state) => ({ scan: { ...state.scan, progress: payload } }))
  },

  setPlanProgress(payload) {
    set((state) => ({ plan: { ...state.plan, progress: payload } }))
  },

  setExecutionProgress(payload) {
    if (payload.stage === "undo") {
      set((state) => ({ undo: { ...state.undo, progress: payload } }))
    } else {
      set((state) => ({ execution: { ...state.execution, progress: payload } }))
    }
  },

  async runScan() {
    if (get().scan.status === "running") {
      return
    }

    set({
      scan: runningOperationState<ScanProgressPayload, ScanSummary>(),
      plan: initialOperationState<PlanProgressPayload, PlanSummary>(),
      execution: initialOperationState<ExecutionProgressPayload, ExecutionSummary>(),
      undo: initialOperationState<ExecutionProgressPayload, UndoSummary>(),
    })

    try {
      const summary = await scanMedia()
      set((state) => ({
        scan: { ...state.scan, status: "success", summary, error: undefined },
      }))
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      set({
        scan: { status: "error", progress: null, summary: null, error: message },
      })
    }
  },

  async generatePlan() {
    if (get().plan.status === "running") {
      return
    }

    set((state) => ({
      plan: runningOperationState<PlanProgressPayload, PlanSummary>(),
      execution: initialOperationState<ExecutionProgressPayload, ExecutionSummary>(),
      undo: initialOperationState<ExecutionProgressPayload, UndoSummary>(),
      scan: { ...state.scan, error: undefined },
    }))

    try {
      const summary = await planTargets()
      set((state) => ({
        plan: { ...state.plan, status: "success", summary, error: undefined },
      }))
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      set({
        plan: { status: "error", progress: null, summary: null, error: message },
      })
    }
  },

  async runExecution(mode, dryRun) {
    if (get().execution.status === "running") {
      return
    }

    set({
      execution: runningOperationState<ExecutionProgressPayload, ExecutionSummary>(),
    })

    try {
      const summary = await executePlan(mode, dryRun)
      set((state) => ({
        execution: { ...state.execution, status: "success", summary, error: undefined },
      }))
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      set({
        execution: { status: "error", progress: null, summary: null, error: message },
      })
    }
  },

  async runUndo() {
    if (get().undo.status === "running") {
      return
    }

    set({
      undo: runningOperationState<ExecutionProgressPayload, UndoSummary>(),
    })

    try {
      const summary = await undoMoves()
      set((state) => ({
        undo: { ...state.undo, status: "success", summary, error: undefined },
      }))
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      set({
        undo: { status: "error", progress: null, summary: null, error: message },
      })
    }
  },

  resetAfterConfig() {
    set({
      scan: initialOperationState<ScanProgressPayload, ScanSummary>(),
      plan: initialOperationState<PlanProgressPayload, PlanSummary>(),
      execution: initialOperationState<ExecutionProgressPayload, ExecutionSummary>(),
      undo: initialOperationState<ExecutionProgressPayload, UndoSummary>(),
    })
  },
}))