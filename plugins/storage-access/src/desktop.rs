use serde::de::DeserializeOwned;
use std::marker::PhantomData;
use tauri::plugin::PluginApi;
use tauri::{AppHandle, Runtime};

pub fn init<R: Runtime, C: DeserializeOwned>(_app: &AppHandle<R>, _api: PluginApi<R, C>) -> StorageAccess<R> {
    StorageAccess(PhantomData)
}

pub struct StorageAccess<R: Runtime>(PhantomData<fn() -> R>);

impl<R: Runtime> StorageAccess<R> {
    pub fn granted(&self) -> crate::Result<bool> {
        Ok(true)
    }

    pub fn request(&self) -> crate::Result<()> {
        Ok(())
    }
}
