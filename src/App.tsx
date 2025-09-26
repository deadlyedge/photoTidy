import { useEffect, type ReactNode } from "react"
import { listen } from "@tauri-apps/api/event"

import { normalizeConfig, CONFIG_BOOTSTRAP_EVENT } from "./services/config"
import { useConfigStore } from "./state/configStore"
import type { RawConfigPayload } from "./types/config"
import "./App.css"

function App() {
  const { config, status, error, bootstrap, setFromEvent } = useConfigStore((state) => ({
    config: state.config,
    status: state.status,
    error: state.error,
    bootstrap: state.bootstrap,
    setFromEvent: state.setFromEvent,
  }))

  useEffect(() => {
    void bootstrap()
    const unlistenPromise = listen<RawConfigPayload>(CONFIG_BOOTSTRAP_EVENT, (event) => {
      setFromEvent(normalizeConfig(event.payload))
    })

    return () => {
      unlistenPromise.then((unlisten) => unlisten()).catch(() => undefined)
    }
  }, [bootstrap, setFromEvent])

  return (
    <main className="app-shell">
      <header className="app-header">
        <h1>photoTidy</h1>
        <p>Migration scaffold with configuration bootstrap and diagnostics.</p>
      </header>
      <section className="app-content">
        {status === "loading" && <StatusBanner>Loading configuration...</StatusBanner>}
        {status === "error" && <StatusBanner kind="error">{error}</StatusBanner>}
        {status === "ready" && config && <ConfigSummary />}
        {status === "idle" && <StatusBanner>Click refresh to load configuration.</StatusBanner>}
      </section>
      <footer className="app-footer">
        <button type="button" onClick={() => void bootstrap()} className="action">
          Refresh configuration
        </button>
      </footer>
    </main>
  )
}

function ConfigSummary() {
  const config = useConfigStore((state) => state.config)

  if (!config) {
    return null
  }

  return (
    <section className="panel">
      <h2>Resolved Paths</h2>
      <dl>
        <InfoItem label="Database">{config.databasePath}</InfoItem>
        <InfoItem label="Image root">{config.imageRoot}</InfoItem>
        <InfoItem label="Output root">{config.outputRoot}</InfoItem>
        <InfoItem label="Duplicates">{config.duplicatesDir}</InfoItem>
        <InfoItem label="Origin JSON">{config.originInfoPath}</InfoItem>
        <InfoItem label="Plan JSON">{config.targetPlanPath}</InfoItem>
        {config.sampleImageRoot && <InfoItem label="Sample images">{config.sampleImageRoot}</InfoItem>}
      </dl>
      <h3>Extensions</h3>
      <p className="chips" role="list">
        {config.imageExtensions.map((ext) => (
          <span key={ext} role="listitem" className="chip">
            {ext}
          </span>
        ))}
      </p>
      <h3>Schema</h3>
      <p>
        Schema version <strong>{config.schemaVersion}</strong>
      </p>
    </section>
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
  kind = "info",
  children,
}: {
  kind?: "info" | "error"
  children: ReactNode
}) {
  return <div className={`status-banner status-${kind}`}>{children}</div>
}

export default App



