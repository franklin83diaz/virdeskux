use anyhow::{Context, Result, anyhow};
use memmap2::MmapMut;
use tempfile::tempfile;
use x11rb::connection::Connection;
use x11rb::protocol::shm as xshm;
use x11rb::protocol::xproto::{ImageFormat, Screen};
use x11rb::rust_connection::RustConnection;

struct ScreenInfo {
    screen: Screen,
    height: u32,
    width: u32,
    shmseg: u32,
    mmap: MmapMut,
    conn: RustConnection,
}

impl ScreenInfo {
    fn new(dpy_name: Option<&str>) -> Result<Self> {
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
        let has_attach_fd =
            ver.major_version > 1 || (ver.major_version == 1 && ver.minor_version >= 2);
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
        let buf_size: usize = stride * (h as usize);

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
        Ok(Self {
            screen: screen.clone(),
            height: h,
            width: w,
            shmseg,
            mmap,
            conn,
        })
    }

    fn capture(&mut self) -> Result<()> {
        let _ = xshm::get_image(
            &self.conn,
            self.screen.root,
            0,
            0,
            self.width as u16,
            self.height as u16,
            !0,                           // plane_mask
            ImageFormat::Z_PIXMAP.into(), // <-- conv a u8
            self.shmseg,
            0, // offset in shm
        )?
        .reply()?;
        self.conn.flush()?;
        Ok(())
    }

    fn cleanup(&self) -> Result<()> {
        xshm::detach(&self.conn, self.shmseg)?;
        self.conn.flush()?;
        Ok(())
    }
}

/// screenshot-lib: Library to capture X11 screens using XShm and XDamage
///
/// parameters:
///  dpy_name: Option<&str> - Name of the X display (e.g., ":1").
/// returns: Vec<u8> - Image data in RGBA format (4 bytes per pixel)
///
/// (c) 2025 Franklin Díaz
pub fn single_capture(dpy_name: Option<&str>) -> Result<MmapMut> {
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
    let buf_size: usize = stride * (h as usize);

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
        for _i in 0..360 {
            result = single_capture(Some(&dpy_name));
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

    #[test]
    fn capture_test_timed() {
        let dpy_name = std::env::var("DISPLAY").unwrap_or(":1".to_string());

        let mut screen_info =
            ScreenInfo::new(Some(&dpy_name)).expect("Failed to create ScreenInfo");
        let mut result = Err(anyhow!("No capture done"));
        let t0 = std::time::Instant::now();
        //test 60 fps
        for _i in 0..60 {
            result = screen_info.capture();
        }
        let duration = t0.elapsed();
        assert!(result.is_ok());
        println!("Captured screen on display {}", dpy_name);
        println!("Buffer length: {}", screen_info.mmap.len());
        println!("Time elapsed in capture() is: {:?}", duration);
        screen_info.cleanup().expect("Failed to cleanup ScreenInfo");
    }
    #[test]
    //For this test run a video or animation for see the changes
    //This test captures every second for 5 seconds
    fn capture_test_img() {
        let dpy_name = std::env::var("DISPLAY").unwrap_or(":1".to_string());

        let mut screen_info =
            ScreenInfo::new(Some(&dpy_name)).expect("Failed to create ScreenInfo");

        let mut img1: Vec<u8> = Vec::new();
        let mut img2: Vec<u8> = Vec::new();

        // Capture every second for 5 seconds
        for _i in 1..=2 {
            if _i == 1 {
                let   result1 = screen_info.capture();
                assert!(result1.is_ok());      
                img1 = screen_info.mmap.iter().copied().collect();
            } else {
                let result2 = screen_info.capture();
                assert!(result2.is_ok());
                img2 = screen_info.mmap.iter().copied().collect();
            }
             
            //sleep 5 seconds
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        println!("Captured screen on display {}", dpy_name);
        //check if both buffers are different
        let diff = img1.iter().zip(img2.iter()).filter(|(a, b)| a != b).count();
        println!("Number of different bytes between captures: {}", diff);
        assert!(diff > 0, "Both captures are identical, no changes detected");
        screen_info.cleanup().expect("Failed to cleanup ScreenInfo");
    }
}
