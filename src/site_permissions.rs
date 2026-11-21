use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Choice {
    pub(crate) container: Option<u64>,
    pub(crate) requesting: String,
    pub(crate) top: String,
    pub(crate) kind: String,
    pub(crate) allowed: bool,
}
impl Choice {
    pub(crate) fn native(&self) -> Option<enzui_webkit::site_permissions::SitePermission> {
        use enzui_webkit::{
            site_permissions::{PermissionOrigin, SitePermission},
            web::policy::PermissionKind,
        };
        Some(SitePermission {
            requesting_origin: PermissionOrigin::parse(&self.requesting)?,
            top_origin: PermissionOrigin::parse(&self.top)?,
            kind: match self.kind.as_str() {
                "camera" => PermissionKind::Camera,
                "microphone" => PermissionKind::Microphone,
                "location" => PermissionKind::Geolocation,
                "notifications" if self.requesting == self.top => PermissionKind::Notifications,
                _ => return None,
            },
        })
    }
}
