use std::str::FromStr;

use nix::unistd::{Group, User};
use serde_with::DeserializeFromStr;

#[derive(DeserializeFromStr, Clone, Debug)]
pub struct UserWrapper(User);

impl FromStr for UserWrapper {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(uid) = u32::from_str(s) {
            if let Ok(Some(user)) = User::from_uid(uid.into()) {
                return Ok(UserWrapper(user));
            };
        };

        if let Ok(Some(user)) = User::from_name(s) {
            return Ok(UserWrapper(user));
        }

        Err("No matching uid or username found")
    }
}

impl From<UserWrapper> for User {
    fn from(value: UserWrapper) -> Self {
        value.0
    }
}

impl FromStr for GroupWrapper {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(gid) = u32::from_str(s) {
            if let Ok(Some(group)) = Group::from_gid(gid.into()) {
                return Ok(GroupWrapper(group));
            };
        };

        if let Ok(Some(group)) = Group::from_name(s) {
            return Ok(GroupWrapper(group));
        }

        Err("No matching gid or groupname found")

    }
}

#[derive(DeserializeFromStr, Clone, Debug)]
pub struct GroupWrapper(Group);

impl From<GroupWrapper> for Group {
    fn from(value: GroupWrapper) -> Self {
        value.0
    }
}
