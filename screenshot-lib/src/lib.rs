use anyhow::{Context, Result, anyhow};
use memmap2::MmapMut;
use tempfile::tempfile;
use x11rb::connection::Connection;
use x11rb::protocol::shm as xshm;
use x11rb::protocol::xproto::ImageFormat;

/// screenshot-lib: Library to capture X11 screens using XShm and XDamage
///
/// parameters:
///  dpy_name: Option<&str> - Name of the X display (e.g., ":1").
/// returns: Vec<u8> - Image data in RGBA format (4 bytes per pixel)
///
/// (c) 2025 Franklin Díaz
pub fn capture(dpy_name: Option<&str>) -> Result<MmapMut> {
    // Connect to X server
    let (conn, screen_num) = x11rb::connect(dpy_name).context("error X11 (DISPLAY)")?;
    let screen = &conn.setup().roots[screen_num];
    // Get screen dimensions
    let (w, h) = (
        screen.width_in_pixels as u32,
        screen.height_in_pixels as u32,
    );

    // Check MIT-SHM version (need >= 1.2 for AttachFd)
    let ver = xshm::query_version(&conn)?.reply()?;
    let has_attach_fd = ver.major_version > 1 || (ver.major_version == 1 && ver.minor_version >= 2);
    if !has_attach_fd {
        return Err(anyhow!("MIT-SHM version 1.2 or higher is required"));
    }

    // 32bpp BGRX
    //
    //  32bpp = 4 bytes per pixel
    let bpp = 4usize;
    // Stride: bytes per row
    let stride = (w as usize) * bpp;
    // Buffer size
    let buf_size = stride * (h as usize);

    // Create shared memory segment
    let file = tempfile().context("failed to create temporary file")?;
    file.set_len(buf_size as u64)
        .context("failed to set length of temporary file")?;
    let mmap = unsafe { MmapMut::map_mut(&file).context("failed to mmap temporary file")? };

    // Create XShm segment
    let shmseg = conn.generate_id()?;
    // Attach shared memory segment to X server
    xshm::attach_fd(&conn, shmseg, file, false)?;
    conn.flush()?;

    let _ = xshm::get_image(
        &conn,
        screen.root,
        0,
        0,
        w as u16,
        h as u16,
        !0,                           // plane_mask
        ImageFormat::Z_PIXMAP.into(), // <-- conv a u8
        shmseg,
        0, // offset in shm
    )?
    .reply()?;

    // Detach and cleanup
    xshm::detach(&conn, shmseg)?;
    conn.flush()?;

    return Ok(mmap);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_test() {
        let dpy_name = std::env::var("DISPLAY").unwrap_or(":1".to_string());
        let t0 = std::time::Instant::now();
        let mut result = Err(anyhow!("No capture done"));
        //test 60 fps
        for _i in 0..60 {
         result = capture(Some(&dpy_name));
        }
        let duration = t0.elapsed();
        assert!(result.is_ok());
        println!("Captured screen on display {}", dpy_name);
        println!("Buffer length: {}", result.as_ref().unwrap().len());
        println!("Time elapsed in capture() is: {:?}", duration);
       // Uncomment to check size (8294400 = 1920*1080*4)
        //let mmap = result.unwrap();
       // assert_eq!(mmap.len(), 8294400);
    }
}
