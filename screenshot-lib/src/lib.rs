use anyhow::{Context, Result, anyhow};
use memmap2::MmapMut;
use tempfile::tempfile;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::damage;
use x11rb::protocol::damage::ConnectionExt as DamageConnectionExt;
use x11rb::protocol::shm as xshm;
use x11rb::protocol::xproto::{ImageFormat, Screen};
use x11rb::rust_connection::RustConnection;

struct ScreenInfo {
    screen: Screen,
    height: u16,
    width: u16,
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
        let (w, h) = (screen.width_in_pixels, screen.height_in_pixels);

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

    /// Capture the screen into the mmap buffer
    ///
    /// parameters:
    ///  x: i16 - X coordinate of the top-left corner of the capture area
    ///  y: i16 - Y coordinate of the top-left corner of the capture area
    ///  w: u16 - Width of the capture area
    ///  h: u16 - Height of the capture area
    ///
    /// returns: Result<()> - Ok on success, Err on failure
    /// the image data is stored in self.mmap in RGBA format (4 bytes per pixel)
    fn capture(&mut self, x: i16, y: i16, w: u16, h: u16) -> Result<()> {
        if w > self.width || h > self.height {
            return Err(anyhow!("Capture dimensions exceed screen size"));
        }

        let _ = xshm::get_image(
            &self.conn,
            self.screen.root,
            x,
            y,
            w,
            h,
            !0,                           // plane_mask
            ImageFormat::Z_PIXMAP.into(), // <-- conv a u8
            self.shmseg,
            0, // offset in shm
        )?
        .reply()?;
        self.conn.flush()?;
        Ok(())
    }

    fn change(&mut self) ->  Result<()> {

        let damage = self.conn.damage_query_version(1, 1)?.reply()?.major_version;
        println!("Damage version: {}", damage);

        let damage_id = self.conn.generate_id().context("failed to generate damage ID")?;
        self.conn.damage_create(
            damage_id,
            self.screen.root,
            damage::ReportLevel::DELTA_RECTANGLES,
        )?;
        self.conn.flush()?;
        let mut i=0;
        loop {
            let event = self.conn.wait_for_event().context("failed to wait for event")?;
            i += 1; if i > 10 { break; } // Avoid infinite loop in tests
            match event {
                Event::DamageNotify(ev) => {
                    self.conn.damage_subtract(damage_id, x11rb::NONE, x11rb::NONE).context("failed to subtract damage")?;
                    self.conn.flush().context("failed to flush connection")?;
                    let area = ev.area;
                    println!(
                        "Área cambiada: {} x {} to {} x {} ",
                        area.x, area.y, area.width, area.height
                    );
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn cleanup(&self) -> Result<()> {
        xshm::detach(&self.conn, self.shmseg)?;
        self.conn.flush()?;
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_test_timed() {
        let dpy_name = std::env::var("DISPLAY").unwrap_or(":1".to_string());

        let mut screen_info =
            ScreenInfo::new(Some(&dpy_name)).expect("Failed to create ScreenInfo");
        let mut result = Err(anyhow!("No capture done"));
        let t0 = std::time::Instant::now();
        //test 60 fps
        for _i in 0..60 {
            result = screen_info.capture(0, 0, 1000, 1000);
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
    //This test captures every second for 1 seconds
    fn capture_test_img() {
        let dpy_name = std::env::var("DISPLAY").unwrap_or(":1".to_string());

        let mut screen_info =
            ScreenInfo::new(Some(&dpy_name)).expect("Failed to create ScreenInfo");

        let mut img1: Vec<u8> = Vec::new();
        let mut img2: Vec<u8> = Vec::new();

        // Capture every second for 1 seconds
        for _i in 1..=2 {
            if _i == 1 {
                let result1 = screen_info.capture(0, 0, 1000, 1000);
                assert!(result1.is_ok());
                img1 = screen_info.mmap.iter().copied().collect();
            } else {
                let result2 = screen_info.capture(0, 0, 1000, 1000);
                assert!(result2.is_ok());
                img2 = screen_info.mmap.iter().copied().collect();
            }

            //sleep 1 seconds
            std::thread::sleep(std::time::Duration::from_secs(1));
        }

        println!("Captured screen on display {}", dpy_name);
        //check if both buffers are different
        let diff = img1.iter().zip(img2.iter()).filter(|(a, b)| a != b).count();
        println!("Number of different bytes between captures: {}", diff);
        assert!(diff > 0, "Both captures are identical, no changes detected");
        screen_info.cleanup().expect("Failed to cleanup ScreenInfo");
    }

        #[test]
    fn changed_test() {
        let dpy_name = std::env::var("DISPLAY").unwrap_or(":1".to_string());

        let mut screen_info =
            ScreenInfo::new(Some(&dpy_name)).expect("Failed to create ScreenInfo");
        let result = screen_info.change();
        assert!(result.is_ok());
        screen_info.cleanup().expect("Failed to cleanup ScreenInfo");
    }
}
