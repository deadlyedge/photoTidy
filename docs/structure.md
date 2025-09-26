~~~mermaid
flowchart TD
    UI[React UI] -->|bootstrap_paths| Config[Config Service]
    Config -->|persist paths| SQLite[(SQLite DB)]
    UI -->|scan_media request| ScanWorker[Scan Worker]
    ScanWorker -->|enumerate & hash| FS[(Filesystem)]
    ScanWorker -->|write inventory| SQLite
    UI -->|view progress| Events[Tauri Event Bus]
    ScanWorker -->|emit scan->diff->hash| Events
    UI -->|plan_targets request| Planner[Planning Engine]
    Planner -->|read inventory| SQLite
    Planner -->|write plan entries| SQLite
    UI -->|execute_plan| Executor[Execution Engine]
    Executor -->|copy/move files| FS
    Executor -->|append op log| SQLite
    Executor -->|emit status| Events
    Events -->|update UI state| UI
~~~
