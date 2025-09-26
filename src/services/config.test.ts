import { describe, expect, it } from "vitest"

import { normalizeConfig } from "./config"

describe("normalizeConfig", () => {
  it("converts snake_case payload to camelCase", () => {
    const payload = {
      schema_version: 1,
      database_path: "/tmp/db.sqlite3",
      image_root: "/images/",
      image_root_default_name: "images",
      output_root: "/output/",
      output_root_name: "output",
      duplicates_dir: "/output/duplicates/",
      duplicates_folder_name: "duplicates",
      origin_info_json: "/output/origin.json",
      target_plan_json: "/output/plan.json",
      image_exts: [".jpg", ".png"],
      sample_image_root: null,
    }

    const result = normalizeConfig(payload)
    expect(result.schemaVersion).toBe(1)
    expect(result.imageExtensions).toEqual([".jpg", ".png"])
    expect(result.sampleImageRoot).toBeUndefined()
  })
})
