//! Android "All files access" (MANAGE_EXTERNAL_STORAGE): needed to measure
//! folders in shared storage. Desktop platforms always have access.

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

mod error;
pub use error::{Error, Result};

#[cfg(desktop)]
mod desktop;
#[cfg(desktop)]
use desktop::StorageAccess;

#[cfg(mobile)]
mod mobile;
#[cfg(mobile)]
use mobile::StorageAccess;

pub trait StorageAccessExt<R: Runtime> {
    fn storage_access(&self) -> &StorageAccess<R>;
}

impl<R: Runtime, T: Manager<R>> StorageAccessExt<R> for T {
    fn storage_access(&self) -> &StorageAccess<R> {
        self.state::<StorageAccess<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("storage-access")
        .setup(|app, api| {
            #[cfg(mobile)]
            let access = mobile::init(app, api)?;
            #[cfg(desktop)]
            let access = desktop::init(app, api);
            app.manage(access);
            Ok(())
        })
        .build()
}
