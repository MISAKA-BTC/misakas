//! One solved child per selected parent in an operator-controlled historical run.
//! The journal is only a pointer. The native block store determines whether a retry is needed.
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

pub(super) fn read(permit: &Path, parent: [u8; 64]) -> io::Result<Option<[u8; 64]>> {
    let mut file = match fs::File::open(permit.with_extension("submitted-child")) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    if file.metadata()?.len() != 128 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid submitted-child record"));
    }
    let mut recorded_parent = [0; 64];
    let mut child = [0; 64];
    file.read_exact(&mut recorded_parent)?;
    file.read_exact(&mut child)?;
    Ok((recorded_parent == parent).then_some(child))
}

pub(super) fn write(permit: &Path, parent: [u8; 64], child: [u8; 64]) -> io::Result<()> {
    let destination = permit.with_extension("submitted-child");
    let temporary = permit.with_extension("submitted-child.tmp");
    let mut bytes = [0; 128];
    bytes[..64].copy_from_slice(&parent);
    bytes[64..].copy_from_slice(&child);
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewritten_permit_at_same_parent_retains_child_across_reopen() {
        let dir = std::env::temp_dir().join(format!("t12-submitted-child-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let permit = dir.join("heartbeat-permit.json");
        assert_eq!(read(&permit, [1; 64]).unwrap(), None);
        write(&permit, [1; 64], [2; 64]).unwrap();
        fs::write(&permit, b"new permit for same parent").unwrap();
        assert_eq!(read(&permit, [1; 64]).unwrap(), Some([2; 64]));
        assert_eq!(read(&permit, [2; 64]).unwrap(), None);
        write(&permit, [2; 64], [3; 64]).unwrap();
        assert_eq!(read(&permit, [2; 64]).unwrap(), Some([3; 64]));
        fs::write(permit.with_extension("submitted-child"), [0; 5]).unwrap();
        assert_eq!(read(&permit, [2; 64]).unwrap_err().kind(), io::ErrorKind::InvalidData);
        fs::remove_dir_all(dir).unwrap();
    }
}
