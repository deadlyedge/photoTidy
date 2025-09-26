import { create } from "zustand"

import type { AppConfig } from "../types/config"
import { fetchBootstrapConfig } from "../services/config"

type Status = "idle" | "loading" | "ready" | "error"

interface ConfigState {
  status: Status
  config: AppConfig | null
  error?: string
  bootstrap: () => Promise<void>
  setFromEvent: (config: AppConfig) => void
}

export const useConfigStore = create<ConfigState>((set, get) => ({
  status: "idle",
  config: null,
  error: undefined,
  async bootstrap() {
    const { status } = get()
    if (status === "loading") {
      return
    }

    set({ status: "loading", error: undefined })
    try {
      const config = await fetchBootstrapConfig()
      set({ config, status: "ready" })
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      set({ status: "error", error: message })
    }
  },
  setFromEvent(config) {
    set({ config, status: "ready", error: undefined })
  },
}))
