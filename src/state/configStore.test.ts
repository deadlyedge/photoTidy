import { act } from "react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { useConfigStore } from "./configStore"
import { fetchBootstrapConfig } from "../services/config"

vi.mock("../services/config", () => ({
  fetchBootstrapConfig: vi.fn(),
}))

const mockedFetch = vi.mocked(fetchBootstrapConfig)

describe("config store", () => {
  beforeEach(() => {
    useConfigStore.setState({ status: "idle", config: null, error: undefined })
    mockedFetch.mockReset()
  })

  it("loads configuration", async () => {
    const payload = {
      schemaVersion: 1,
      databasePath: "/db",
      imageRoot: "/images/",
      imageRootDefaultName: "images",
      outputRoot: "/output/",
      outputRootName: "output",
      duplicatesDir: "/output/dup/",
      duplicatesFolderName: "dup",
      originInfoPath: "/origin.json",
      targetPlanPath: "/plan.json",
      imageExtensions: [".jpg"],
      sampleImageRoot: undefined,
    }
    mockedFetch.mockResolvedValue(payload)

    await act(async () => {
      await useConfigStore.getState().bootstrap()
    })

    const state = useConfigStore.getState()
    expect(state.status).toBe("ready")
    expect(state.config).toEqual(payload)
  })
})

