use std::{error::Error, fmt};
use std::mem;
use std::mem::MaybeUninit;
use std::os::raw::c_char;
use std::ptr::NonNull;
use std::sync::{Once, ONCE_INIT};

mod ffi;
mod settings;

fn fourcc(a: char, b: char, c: char, d: char) -> u32 {
    let a: u32 = a as u32;
    let b: u32 = b as u32;
    let c: u32 = c as u32;
    let d: u32 = d as u32;

    a | (b << 8) | (c << 16) | (d << 24)
}

// BAH
const MMAL_CAMERA_PREVIEW_PORT: isize = 0;
const MMAL_CAMERA_VIDEO_PORT: isize = 1;
const MMAL_CAMERA_CAPTURE_PORT: isize = 2;

// TODO: hoping the value of opaque is 0. couldn't find def in raspi userland repo
const MMAL_ENCODING_OPAQUE: u32 = 0;
const MMAL_ENCODING_JPEG: u32 = fourcc('J', 'P', 'E', 'G');


struct CameraError {
    code: i32,
    message: String
}
impl fmt::Display for CameraError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let err_msg = match self.code {
            1 => "Something wrong",
            404 => "Not Found",
            _ => "Generic error"
        };
        write!(f, "{}", err_msg)
    }
}
impl fmt::Debug for CameraError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "CameraError {{ code: {}, message: {} }}",
            self.code, self.message
        )
    }
}


fn camera_port_callback(port: *mut ffi::MMAL_PORT_T, buffer: *mut ffi::MMAL_BUFFER_HEADER_T) {
    println!("camera port callback");
    unsafe { ffi::mmal_buffer_header_release(buffer); }
}


/*
TODO: could I use an enum for each of these componenets?

The idea is that Camera could have variants for unitialized, pending, ready, etc. maybe?
That might have some tricy edge cases, but I probably don't really understand how enums work.
*/
struct Camera {
    camera: NonNull<ffi::MMAL_COMPONENT_T>,
    camera_enabled: bool,
    encoder: NonNull<ffi::MMAL_COMPONENT_T>,
    encoder_enabled: bool
}

impl Camera {
    pub fn new() -> Result<Camera, CameraError> {

        // LEARNING: asterisk create a mutable raw pointer type
        let mut camera_ptr = MaybeUninit::<*mut ffi::MMAL_COMPONENT_T>::uninit();
        let component: *const c_char = ffi::MMAL_COMPONENT_DEFAULT_CAMERA.as_ptr() as *const c_char;
        let status = unsafe { ffi::mmal_component_create(component, camera_ptr.as_mut_ptr()) };

        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Failed to initialize camera".to_string()
            })
        }
        let camera_ptr: *mut ffi::MMAL_COMPONENT_T = unsafe { camera_ptr.assume_init() };
        let camera = NonNull::new(camera_ptr).unwrap();
       
        
        // choose which camera port to read from
        let mut param: ffi::MMAL_PARAMETER_INT32_T = unsafe { mem::zeroed() };
        param.hdr.id = ffi::MMAL_PARAMETER_CAMERA_NUM as u32;
        param.hdr.size = mem::size_of::<ffi::MMAL_PARAMETER_INT32_T>() as u32;
        param.value = 0 as i32; // believe this chooses the default camera
        let status = unsafe { ffi::mmal_port_parameter_set(camera.as_ref().control, &param.hdr) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to set camera number".to_string()
            })
        }

        
        // set sensor mode to 0 which is auto
        let status = unsafe { ffi::mmal_port_parameter_set_uint32(camera.as_ref().control, ffi::MMAL_PARAMETER_CAMERA_CUSTOM_SENSOR_CONFIG, 0) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to set sensor mode".to_string()
            })
        }
        
        // TODO: figure out how to use rust callback with C library
        /*
        // Enable camera port and pass it the callback function
        let status = unsafe { ffi::mmal_port_enable(camera.as_ref().control, Some(camera_port_callback)) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to enable camera".to_string()
            })
        }
        */
        
        // Set up camera configuration
        /*
        enabled: false,
        camera_port_enabled: false,
        pool: None,
        mutex: Arc::new(Mutex::new(())),
        still_port_enabled: false,
        // this is really a hack. ideally these objects wouldn't be structured this way
        encoder_created: false,
        encoder_enabled: false,
        encoder_control_port_enabled: false,
        encoder_output_port_enabled: false,
        encoder: None,
        connection_created: false,
        connection: None,
        preview_created: false,
        preview: None,
        use_encoder: false,
        */

        let w = 800;
        let h = 600;
        
        let mut cfg: ffi::MMAL_PARAMETER_CAMERA_CONFIG_T = unsafe { mem::zeroed() };
        cfg.hdr.id = ffi::MMAL_PARAMETER_CAMERA_CONFIG as u32;
        cfg.hdr.size = mem::size_of::<ffi::MMAL_PARAMETER_CAMERA_CONFIG_T>() as u32;
        
        // https://github.com/raspberrypi/userland/blob/master/host_applications/linux/apps/raspicam/RaspiStillYUV.c#L706
        cfg.max_stills_w = w;
        cfg.max_stills_h = h;
        cfg.stills_yuv422 = 0;
        cfg.one_shot_stills = 1;
        cfg.max_preview_video_w = w;
        cfg.max_preview_video_h = h;
        cfg.num_preview_video_frames = 1;
        cfg.stills_capture_circular_buffer_height = 0;
        cfg.fast_preview_resume = 0;
        cfg.use_stc_timestamp = ffi::MMAL_PARAMETER_CAMERA_CONFIG_TIMESTAMP_MODE_T_MMAL_PARAM_TIMESTAMP_MODE_RESET_STC;
        
        let status = unsafe { ffi::mmal_port_parameter_set(camera.as_ref().control, &cfg.hdr) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            // shutdown camera
            return Err(CameraError {
                code: 1,
                message: "Unable to set camera settings".to_string()
            })
        }
        
        // raspistill then sets saturation, sharpness, etc ....
        
        
        
        let camera_outputs = camera.as_ref().output;
        
        
        let still_port_ptr = *(camera_outputs.offset(MMAL_CAMERA_CAPTURE_PORT) as *mut *mut ffi::MMAL_PORT_T);
        let mut still_port = *still_port_ptr;
        let mut format = still_port.format;
        
        // https://github.com/raspberrypi/userland/blob/master/host_applications/linux/apps/raspicam/RaspiStillYUV.c#L799
        
        //if self.use_encoder {
            (*format).encoding = MMAL_ENCODING_OPAQUE;
            /*
        } else {
            (*format).encoding = encoding;
            (*format).encoding_variant = 0; //Irrelevant when not in opaque mode
        }
        */
        
        // es = elementary stream
        let es = (*format).es;
        
        (*es).video.width = w & !(32-1); // VCOS_ALIGN_UP ffi::vcos_align_up(w, 32);
        (*es).video.height = (h + 16 - 1) & !(16-1); // ffi::vcos_align_up(h, 16);
        (*es).video.crop.x = 0;
        (*es).video.crop.y = 0;
        (*es).video.crop.width = w as i32;
        (*es).video.crop.height = h as i32;
        (*es).video.frame_rate.num = 0; //STILLS_FRAME_RATE_NUM;
        (*es).video.frame_rate.den = 1; //STILLS_FRAME_RATE_DEN;
        
        let status = unsafe { ffi::mmal_port_format_commit(still_port_ptr) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to commit still port configuration".to_string()
            })
        }
        
        let mut encoder_ptr = MaybeUninit::uninit();
        let encoder = NonNull::new(encoder_ptr).unwrap();
        return Ok(Camera {
            camera: camera,
            // TODO: i don't like that we need all these flags. wish we could embed them in the camera type/value
            camera_enabled: false,

            encoder: encoder,
            encoder_enabled: false
        });
        
        // Configure the camera
        /*
        
        // TODO: should this be before or after the commit?
        if still_port.buffer_size < still_port.buffer_size_min {
            still_port.buffer_size = still_port.buffer_size_min;
        }
        
        still_port.buffer_num = still_port.buffer_num_recommended;
        
        
        
        
        
        
        // create encoder ... maybe jpeg for now
        let component: *const c_char = ffi::MMAL_COMPONENT_DEFAULT_IMAGE_ENCODER.as_ptr() as *const c_char;
        let status = unsafe { ffi::mmal_component_create(component, encoder_ptr.as_mut_ptr()) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to create encoder component".to_string()
            })
        }
        let encoder_ptr: *mut ffi::MMAL_COMPONENT_T = unsafe { encoder_ptr.assume_init() };
        // TODO: what does this do?
        let encoder = NonNull::new(encoder_ptr).unwrap();
        


        let settings = settings::CameraSettings::default();





        let mut encoding = MMAL_ENCODING_JPEG;

        let output = camera.as_ref().output;
       
        //let output_num = self.camera.as_ref().output_num;
        //assert_eq!(output_num, 3, "Expected camera to have 3 outputs");
       

        let video_port_ptr = *(output.offset(MMAL_CAMERA_VIDEO_PORT) as *mut *mut ffi::MMAL_PORT_T);
        let mut video_port = *video_port_ptr;

        // On firmware prior to June 2016, camera and video_splitter
        // had BGR24 and RGB24 support reversed.
        if encoding == ffi::MMAL_ENCODING_RGB24 || encoding == ffi::MMAL_ENCODING_BGR24 {
            encoding = if ffi::mmal_util_rgb_order_fixed(still_port_ptr) == 1 {
                ffi::MMAL_ENCODING_RGB24
            } else {
                ffi::MMAL_ENCODING_BGR24
            };
        }

        let control = camera.as_ref().control;

        // TODO:
        //raspicamcontrol_set_all_parameters(camera, &state->camera_parameters);

        let status = ffi::mmal_port_parameter_set_uint32(control, ffi::MMAL_PARAMETER_ISO, settings.iso);
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to set ISO".to_string()
            });
        }

        

        let enable_zero_copy = if settings.zero_copy {
            ffi::MMAL_TRUE
        } else {
            ffi::MMAL_FALSE
        };
        let status = ffi::mmal_port_parameter_set_boolean(
            video_port_ptr,
            ffi::MMAL_PARAMETER_ZERO_COPY as u32,
            enable_zero_copy as i32,
        );

        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to set zero copy to".to_string()
            });
            //                format!("Unable to set zero copy to {}", settings.zero_copy),
        }

        let status = ffi::mmal_port_format_commit(still_port_ptr);
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to set still port format".to_string()
            });
        }

        if !self.use_encoder {
            return Ok();
        }

        let encoder_in_port_ptr =
            *(self.encoder.unwrap().as_ref().input.offset(0) as *mut *mut ffi::MMAL_PORT_T);
        let encoder_out_port_ptr =
            *(self.encoder.unwrap().as_ref().output.offset(0) as *mut *mut ffi::MMAL_PORT_T);
        let encoder_in_port = *encoder_in_port_ptr;
        let mut encoder_out_port = *encoder_out_port_ptr;

        // We want same format on input and output
        ffi::mmal_format_copy(encoder_out_port.format, encoder_in_port.format);

        format = encoder_out_port.format;
        (*format).encoding = encoding;

        encoder_out_port.buffer_size = encoder_out_port.buffer_size_recommended;
        if encoder_out_port.buffer_size < encoder_out_port.buffer_size_min {
            encoder_out_port.buffer_size = encoder_out_port.buffer_size_min;
        }

        encoder_out_port.buffer_num = encoder_out_port.buffer_num_recommended;
        if encoder_out_port.buffer_num < encoder_out_port.buffer_num_min {
            encoder_out_port.buffer_num = encoder_out_port.buffer_num_min;
        }

        let status = ffi::mmal_port_format_commit(encoder_out_port_ptr);
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to set encoder output port format".to_string()
            });
        }

        if encoding == MMAL_ENCODING_JPEG || encoding == ffi::MMAL_ENCODING_MJPEG {
            // Set the JPEG quality level
            let status = ffi::mmal_port_parameter_set_uint32(
                encoder_out_port_ptr,
                ffi::MMAL_PARAMETER_JPEG_Q_FACTOR,
                90,
            );
            if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
                return Err(CameraError {
                    code: 1,
                    message: "Unable to set JPEG quality".to_string()
                });
            }

            // Set the JPEG restart interval
            let status = ffi::mmal_port_parameter_set_uint32(
                encoder_out_port_ptr,
                ffi::MMAL_PARAMETER_JPEG_RESTART_INTERVAL,
                0,
            );
            if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
                return Err(CameraError {
                    code: 1,
                    message: "Unable to set JPEG restart interval".to_string()
                });
            }
        }



        */

        



        /*


    

        // camera.set_camera_format(MMAL_ENCODING_JPEG, self.info.max_width, self.info.max_height, false)?;
        camera.set_camera_format(se
            ttings)?;
        // OH GOD THIS ONE IS GOING TO TAKE A WHILE

        
        camera.enable_control_port(false)?;

        camera.enable()?;
        camera.enable_encoder()?; // only needed if processing image eg returning jpeg
        camera.create_pool()?;

        //camera.connect_preview()?;
        // camera.enable_preview()?;

        camera.connect_encoder()?;
        */
    }

    fn disable_camera(&self) {
        if self.camera_enabled {
            unsafe {
                ffi::mmal_component_disable(self.camera.as_ptr());
            }
        }
    }
    fn destroy_camera(&self) {
        unsafe {
            ffi::mmal_component_destroy(self.camera.as_ptr());
        }
    }
    fn disable_encoder(&self) {
        if self.encoder_enabled {
            unsafe {
                ffi::mmal_component_disable(self.encoder.as_ptr());
            }
        }
    }
    fn destroy_encoder(&self) {
        unsafe {
            ffi::mmal_component_destroy(self.encoder.as_ptr());
        }
    }

    pub fn shutdown(&self) {
        unsafe {
            //ffi::mmal_connection_disable(self.connection.unwrap().as_ptr());
            //ffi::mmal_connection_destroy(self.connection.unwrap().as_ptr());
            
            ffi::mmal_component_disable(self.encoder.as_ptr());

            self.disable_camera();
            
            ffi::mmal_port_disable(self.encoder.as_ref().control);
            
            ffi::mmal_port_disable(self.camera.as_ref().control);
        }
            
        self.destroy_camera();
        self.destroy_encoder();

    }
}

fn main() {
    unsafe {
        ffi::bcm_host_init();
        ffi::vcos_init();
        ffi::mmal_vc_init();
    }

    let mut camera = Camera::new().unwrap();

    println!("Hey");

    camera.shutdown();




}
