use std::panic::catch_unwind;
use std::path::PathBuf;
use std::sync::mpsc::channel;

#[tauri::command]
pub async fn start_native_drag(
    app: tauri::AppHandle,
    window: tauri::Window,
    paths: Vec<String>,
    icon: String,
) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }

    let (tx, rx) = channel::<Result<(), String>>();

    let items: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    let image = drag::Image::File(PathBuf::from(icon));

    app.run_on_main_thread(move || {
        let result = catch_unwind(std::panic::AssertUnwindSafe(|| {
            drag::start_drag(
                &window,
                drag::DragItem::Files(items),
                image,
                |_result, _cursor_pos| {},
                Default::default(),
            )
        }));

        let mapped = match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(format!("drag error: {e}")),
            Err(_) => Err("native drag session failed (NULL from macOS)".into()),
        };
        let _ = tx.send(mapped);
    })
    .map_err(|e| format!("run_on_main_thread: {e}"))?;

    rx.recv().map_err(|_| "drag channel closed".to_string())?
}
