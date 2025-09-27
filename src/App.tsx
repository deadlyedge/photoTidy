import { confirm } from '@tauri-apps/plugin-dialog'
// import { confirm } from '@tauri-apps/api/dialog'
import { listen } from '@tauri-apps/api/event'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'

import { normalizeConfig, CONFIG_BOOTSTRAP_EVENT } from './services/config'
import { EXECUTION_PROGRESS_EVENT, PLAN_PROGRESS_EVENT } from './services/plan'
import { SCAN_PROGRESS_EVENT } from './services/scan'
import { checkDiskSpace } from './services/system'
import { useConfigStore } from './state/configStore'
import { useWorkflowStore, type StageStatus } from './state/workflowStore'
import type { ExecutionMode, PlanItem, PlanProgressPayload } from './types/plan'
import type { DiskStatus } from './types/system'
import type { RawConfigPayload } from './types/config'
import type { ExecutionProgressPayload } from './types/plan'
import type { ScanProgressPayload } from './types/scan'
import './App.css'

function App() {
  const { config, status, error, bootstrap, setFromEvent } = useConfigStore(
    (state) => ({
      config: state.config,
      status: state.status,
      error: state.error,
      bootstrap: state.bootstrap,
      setFromEvent: state.setFromEvent,
    }),
  )

  const {
    scan,
    plan,
    execution,
    undo,
    setScanProgress,
    setPlanProgress,
    setExecutionProgress,
    runScan,
    generatePlan,
    runExecution,
    runUndo,
    resetAfterConfig,
  } = useWorkflowStore((state) => ({
    scan: state.scan,
    plan: state.plan,
    execution: state.execution,
    undo: state.undo,
    setScanProgress: state.setScanProgress,
    setPlanProgress: state.setPlanProgress,
    setExecutionProgress: state.setExecutionProgress,
    runScan: state.runScan,
    generatePlan: state.generatePlan,
    runExecution: state.runExecution,
    runUndo: state.runUndo,
    resetAfterConfig: state.resetAfterConfig,
  }))

  const [executionMode, setExecutionMode] = useState<ExecutionMode>('copy')
  const [dryRun, setDryRun] = useState(true)
  const [diskStatus, setDiskStatus] = useState<DiskStatus | null>(null)

  useEffect(() => {
    void bootstrap()
    const unlisten = listen<RawConfigPayload>(
      CONFIG_BOOTSTRAP_EVENT,
      (event) => {
        setFromEvent(normalizeConfig(event.payload))
      },
    )

    return () => {
      unlisten.then((fn) => fn()).catch(() => undefined)
    }
  }, [bootstrap, setFromEvent])

  useEffect(() => {
    const subscriptions: Array<Promise<UnlistenFn>> = [
      listen<ScanProgressPayload>(SCAN_PROGRESS_EVENT, (event) => {
        setScanProgress(event.payload)
      }),
      listen<PlanProgressPayload>(PLAN_PROGRESS_EVENT, (event) => {
        setPlanProgress(event.payload)
      }),
      listen<ExecutionProgressPayload>(EXECUTION_PROGRESS_EVENT, (event) => {
        setExecutionProgress(event.payload)
      }),
    ]

    return () => {
      subscriptions.forEach((promise) => {
        promise.then((unlisten) => unlisten()).catch(() => undefined)
      })
    }
  }, [setExecutionProgress, setPlanProgress, setScanProgress])

  const configFingerprint = useRef<string | null>(null)
  useEffect(() => {
    if (status !== 'ready' || !config) {
      return
    }
    const fingerprint = [
      config.databasePath,
      config.imageRoot,
      config.outputRoot,
    ].join('|')
    if (configFingerprint.current !== fingerprint) {
      resetAfterConfig()
      configFingerprint.current = fingerprint
    }
  }, [status, config, resetAfterConfig])

  const planSummary = plan.summary
  const planBuckets = useMemo(() => {
    if (!planSummary) {
      return []
    }
    const groups = new Map<
      string,
      {
        path: string
        items: PlanItem[]
        duplicateCount: number
        totalSize: number
      }
    >()
    for (const item of planSummary.entries) {
      const bucket = groups.get(item.newPath) ?? {
        path: item.newPath,
        items: [],
        duplicateCount: 0,
        totalSize: 0,
      }
      bucket.items.push(item)
      bucket.totalSize += item.fileSize
      if (item.isDuplicate) {
        bucket.duplicateCount += 1
      }
      groups.set(item.newPath, bucket)
    }
    return Array.from(groups.values()).sort((a, b) =>
      a.path.localeCompare(b.path),
    )
  }, [planSummary])

  const canGeneratePlan = scan.status === 'success' && scan.summary !== null
  const canExecute = plan.status === 'success' && planSummary !== null
  const canUndoMoves =
    execution.summary?.mode === 'move' && execution.summary?.dryRun === false

  const handleExecute = async () => {
    if (!planSummary) {
      return
    }

    let latestDisk: DiskStatus | null = null
    try {
      latestDisk = await checkDiskSpace()
      setDiskStatus(latestDisk)
    } catch (err) {
      console.warn('Failed to check disk space', err)
    }

    if (
      !dryRun &&
      latestDisk &&
      planSummary.totalBytes > latestDisk.availableBytes
    ) {
      const proceedOnSpace = await confirm(
        `Only ${formatBytes(latestDisk.availableBytes)} free on ${latestDisk.path}, but the plan needs ${formatBytes(planSummary.totalBytes)}. Continue anyway?`,
        { title: 'Disk space warning', kind: 'warning' },
      )
      if (!proceedOnSpace) {
        return
      }
    }

    const proceed = await confirm(
      `Ready to ${dryRun ? 'dry run' : executionMode} ${planSummary.totalEntries} planned files?`,
      {
        title: dryRun ? 'Confirm dry run' : 'Confirm execution',
        kind: dryRun ? 'info' : executionMode === 'move' ? 'warning' : 'info',
      },
    )

    if (!proceed) {
      return
    }

    await runExecution(executionMode, dryRun)
  }

  const handleUndo = async () => {
    const proceed = await confirm(
      'Undo recently moved files and restore originals?',
      {
        title: 'Undo moves',
        kind: 'warning',
      },
    )

    if (!proceed) {
      return
    }

    await runUndo()
  }

  return (
    <main className="app-shell">
      <header className="app-header">
        <h1>photoTidy</h1>
        <p>Plan, preview, and execute tidy operations with live progress.</p>
      </header>

      <section className="workflow">
        <WorkflowStep title="1. Configuration" status={statusToDisplay(status)}>
          <div className="step-body">
            {status === 'loading' && (
              <StatusBanner>Loading configuration…</StatusBanner>
            )}
            {status === 'error' && error && (
              <StatusBanner kind="error">{error}</StatusBanner>
            )}
            {status === 'ready' && config && (
              <ConfigSummary
                config={config}
                onRefresh={() => void bootstrap()}
              />
            )}
            {status === 'idle' && (
              <StatusBanner>Click refresh to load configuration.</StatusBanner>
            )}
          </div>
        </WorkflowStep>

        <WorkflowStep
          title="2. Scan Media"
          status={scan.status}
          actions={
            <button
              type="button"
              className="action"
              onClick={() => void runScan()}
              disabled={scan.status === 'running' || status !== 'ready'}
            >
              {scan.status === 'running' ? 'Scanning…' : 'Start scan'}
            </button>
          }
        >
          <OperationProgress
            operation={scan}
            progressLabel={(payload) => formatScanProgress(payload)}
          />
          {scan.summary && (
            <ul className="metrics">
              <li>Total files: {scan.summary.totalFiles}</li>
              <li>Hashed this run: {scan.summary.hashedFiles}</li>
              <li>Reused from cache: {scan.summary.skippedFiles}</li>
              <li>Duplicates flagged: {scan.summary.duplicateFiles}</li>
            </ul>
          )}
        </WorkflowStep>

        <WorkflowStep
          title="3. Plan Targets"
          status={plan.status}
          actions={
            <button
              type="button"
              className="action"
              onClick={() => void generatePlan()}
              disabled={!canGeneratePlan || plan.status === 'running'}
            >
              {plan.status === 'running' ? 'Planning…' : 'Generate plan'}
            </button>
          }
        >
          <OperationProgress
            operation={plan}
            progressLabel={(payload) =>
              `${payload.processed}/${payload.total} items`
            }
          />
          {planSummary && (
            <div className="plan-overview">
              <ul className="metrics">
                <li>Total planned: {planSummary.totalEntries}</li>
                <li>Duplicates: {planSummary.duplicateEntries}</li>
                <li>Unique: {planSummary.uniqueEntries}</li>
                <li>Destination folders: {planSummary.destinationBuckets}</li>
                <li>Estimated size: {formatBytes(planSummary.totalBytes)}</li>
              </ul>
              <PlanPreview buckets={planBuckets} />
            </div>
          )}
        </WorkflowStep>

        <WorkflowStep
          title="4. Execute"
          status={execution.status}
          actions={
            <div className="execute-actions">
              <label className="radio">
                <input
                  type="radio"
                  name="execution-mode"
                  value="copy"
                  checked={executionMode === 'copy'}
                  onChange={() => setExecutionMode('copy')}
                />
                Copy
              </label>
              <label className="radio">
                <input
                  type="radio"
                  name="execution-mode"
                  value="move"
                  checked={executionMode === 'move'}
                  onChange={() => setExecutionMode('move')}
                />
                Move
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={dryRun}
                  onChange={(event) => setDryRun(event.target.checked)}
                />{' '}
                Dry run
              </label>
              <button
                type="button"
                className="action"
                onClick={() => void handleExecute()}
                disabled={
                  !canExecute ||
                  execution.status === 'running' ||
                  plan.status !== 'success'
                }
              >
                {execution.status === 'running'
                  ? 'Executing…'
                  : dryRun
                    ? 'Dry run'
                    : `Run ${executionMode}`}
              </button>
              <button
                type="button"
                className="action ghost"
                onClick={() => void handleUndo()}
                disabled={!canUndoMoves || undo.status === 'running'}
              >
                {undo.status === 'running' ? 'Undoing…' : 'Undo moves'}
              </button>
            </div>
          }
        >
          <OperationProgress
            operation={execution}
            progressLabel={(payload) => formatExecutionProgress(payload)}
          />
          {execution.summary && (
            <ul className="metrics">
              <li>Mode: {execution.summary.mode}</li>
              <li>Dry run: {execution.summary.dryRun ? 'Yes' : 'No'}</li>
              <li>Succeeded: {execution.summary.succeeded}</li>
              <li>Failed: {execution.summary.failed}</li>
              <li>Processed: {execution.summary.processedEntries}</li>
              <li>Duplicates touched: {execution.summary.duplicateEntries}</li>
            </ul>
          )}
          {undo.summary && (
            <div className="undo-summary">
              <h4>Undo result</h4>
              <ul className="metrics">
                <li>Restored: {undo.summary.restored}</li>
                <li>Missing at target: {undo.summary.missing}</li>
                <li>Failures: {undo.summary.failed}</li>
              </ul>
            </div>
          )}
          {diskStatus && (
            <p className="disk-status">
              Last disk check ({diskStatus.path}):{' '}
              {formatBytes(diskStatus.availableBytes)} free /{' '}
              {formatBytes(diskStatus.totalBytes)} total
            </p>
          )}
        </WorkflowStep>
      </section>
    </main>
  )
}

function ConfigSummary({
  config,
  onRefresh,
}: {
  config: ReturnType<typeof normalizeConfig>
  onRefresh: () => void
}) {
  return (
    <div className="config-summary">
      <dl>
        <InfoItem label="Database">{config.databasePath}</InfoItem>
        <InfoItem label="Image root">{config.imageRoot}</InfoItem>
        <InfoItem label="Output root">{config.outputRoot}</InfoItem>
        <InfoItem label="Duplicates">{config.duplicatesDir}</InfoItem>
        <InfoItem label="Origin JSON">{config.originInfoPath}</InfoItem>
        <InfoItem label="Plan JSON">{config.targetPlanPath}</InfoItem>
        {config.sampleImageRoot && (
          <InfoItem label="Sample images">{config.sampleImageRoot}</InfoItem>
        )}
      </dl>
      <div className="extensions">
        <h3>Extensions</h3>
        <p className="chips" role="list">
          {config.imageExtensions.map((ext) => (
            <span key={ext} role="listitem" className="chip">
              {ext}
            </span>
          ))}
        </p>
      </div>
      <div className="config-actions">
        <span>Schema v{config.schemaVersion}</span>
        <button type="button" onClick={onRefresh} className="action ghost">
          Refresh configuration
        </button>
      </div>
    </div>
  )
}

function WorkflowStep({
  title,
  status,
  actions,
  children,
}: {
  title: string
  status: StageStatus | string
  actions?: ReactNode
  children: ReactNode
}) {
  return (
    <article className="step-card">
      <header className="step-header">
        <div>
          <h2>{title}</h2>
          <StatusPill status={status} />
        </div>
        {actions && <div className="step-actions">{actions}</div>}
      </header>
      <div className="step-content">{children}</div>
    </article>
  )
}

function OperationProgress<P, S>({
  operation,
  progressLabel,
}: {
  operation: {
    status: StageStatus
    progress: P | null
    summary: S | null
    error?: string
  }
  progressLabel: (payload: P) => string
}) {
  const percent = percentageFromProgress(operation.progress)
  const label = operation.progress
    ? progressLabel(operation.progress)
    : operation.status === 'running'
      ? 'Preparing…'
      : 'Idle'

  return (
    <div className="progress-block">
      <div className="progress-track">
        <div className="progress-value" style={{ width: `${percent}%` }} />
      </div>
      <span className="progress-label">{label}</span>
      {operation.status === 'error' && operation.error && (
        <StatusBanner kind="error">{operation.error}</StatusBanner>
      )}
    </div>
  )
}
function PlanPreview({
  buckets,
}: {
  buckets: Array<{
    path: string
    items: PlanItem[]
    duplicateCount: number
    totalSize: number
  }>
}) {
  if (buckets.length === 0) {
    return <StatusBanner>No plan entries yet.</StatusBanner>
  }

  return (
    <div className="plan-preview">
      {buckets.map((bucket) => (
        <details key={bucket.path} open={bucket.duplicateCount > 0}>
          <summary>
            <span className="bucket-path">{bucket.path}</span>
            <span className="bucket-meta">
              {bucket.items.length} files · {bucket.duplicateCount} duplicates ·{' '}
              {formatBytes(bucket.totalSize)}
            </span>
          </summary>
          <ul>
            {bucket.items.map((item) => (
              <li
                key={`${item.fileHash}-${item.newFileName}`}
                className={item.isDuplicate ? 'duplicate' : undefined}
              >
                <span>{item.newFileName}</span>
                {item.isDuplicate && <span className="flag">duplicate</span>}
              </li>
            ))}
          </ul>
        </details>
      ))}
    </div>
  )
}

function InfoItem({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="info-item">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  )
}

function StatusBanner({
  kind = 'info',
  children,
}: {
  kind?: 'info' | 'error'
  children: ReactNode
}) {
  return <div className={`status-banner status-${kind}`}>{children}</div>
}

function StatusPill({ status }: { status: StageStatus | string }) {
  return (
    <span className={`status-pill status-${String(status).toLowerCase()}`}>
      {String(status)}
    </span>
  )
}

function statusToDisplay(status: string): string {
  switch (status) {
    case 'idle':
      return 'idle'
    case 'loading':
      return 'loading'
    case 'error':
      return 'error'
    case 'ready':
      return 'ready'
    default:
      return status
  }
}

function percentageFromProgress(
  payload: { processed: number; total: number } | null,
): number {
  if (!payload || payload.total === 0) {
    return payload && payload.processed > 0 ? 100 : 0
  }
  return Math.min(100, Math.round((payload.processed / payload.total) * 100))
}

function formatScanProgress(progress: ScanProgressPayload): string {
  const stage = progress.stage.toUpperCase()
  const counts = `${progress.processed}/${progress.total}`
  return progress.current
    ? `${stage} ${counts} — ${progress.current}`
    : `${stage} ${counts}`
}

function formatExecutionProgress(progress: ExecutionProgressPayload): string {
  const stage = progress.stage === 'undo' ? 'UNDO' : 'EXECUTE'
  const counts = `${progress.processed}/${progress.total}`
  return progress.current
    ? `${stage} ${counts} — ${progress.current}`
    : `${stage} ${counts}`
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB']
  const exponent = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  )
  const value = bytes / Math.pow(1024, exponent)
  const precision = value >= 10 ? 0 : 1
  return `${value.toFixed(precision)} ${units[exponent]}`
}

export default App
