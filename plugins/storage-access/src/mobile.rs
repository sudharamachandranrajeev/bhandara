use serde::Deserialize;
use serde::de::DeserializeOwned;
use tauri::plugin::{PluginApi, PluginHandle};
use tauri::{AppHandle, Runtime};

pub fn init<R: Runtime, C: DeserializeOwned>(_app: &AppHandle<R>, api: PluginApi<R, C>) -> crate::Result<StorageAccess<R>> {
    #[cfg(target_os = "android")]
    let handle = api.register_android_plugin("dev.bhandara.storageaccess", "StorageAccessPlugin")?;
    #[cfg(target_os = "ios")]
    compile_error!("storage-access has no iOS implementation");
    Ok(StorageAccess(handle))
}

pub struct StorageAccess<R: Runtime>(PluginHandle<R>);

#[derive(Deserialize)]
struct Status {
    granted: bool,
}

impl<R: Runtime> StorageAccess<R> {
    pub fn granted(&self) -> crate::Result<bool> {
        Ok(self.0.run_mobile_plugin::<Status>("status", ())?.granted)
    }

    /// Opens the system screen where the user grants access. Returns at once;
    /// call `granted` again when the app resumes.
    pub fn request(&self) -> crate::Result<()> {
        self.0.run_mobile_plugin::<serde_json::Value>("request", ())?;
        Ok(())
    }
}
