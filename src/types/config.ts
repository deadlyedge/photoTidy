export interface RawConfigPayload {
  schema_version: number
  database_path: string
  image_root: string
  image_root_default_name: string
  output_root: string
  output_root_name: string
  duplicates_dir: string
  duplicates_folder_name: string
  origin_info_json: string
  target_plan_json: string
  image_exts: string[]
  sample_image_root?: string | null
}

export interface AppConfig {
  schemaVersion: number
  databasePath: string
  imageRoot: string
  imageRootDefaultName: string
  outputRoot: string
  outputRootName: string
  duplicatesDir: string
  duplicatesFolderName: string
  originInfoPath: string
  targetPlanPath: string
  imageExtensions: string[]
  sampleImageRoot?: string
}
