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
const MMAL_ENCODING_JPEG: u32 = 1195724874; //fourcc('J', 'P', 'E', 'G');


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

        // BEGIN CAMERA COMPONENT STUFF

        // LEARNING: asterisk create a mutable raw pointer type
        let mut camera_ptr = MaybeUninit::<*mut ffi::MMAL_COMPONENT_T>::uninit();
        let component: *const c_char = ffi::MMAL_COMPONENT_DEFAULT_CAMERA.as_ptr() as *const c_char;
        let status = unsafe { ffi::mmal_component_create(component, camera_ptr.as_mut_ptr()) };

        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Failed to create camera component".to_string()
            })
        }
        let camera_ptr: *mut ffi::MMAL_COMPONENT_T = unsafe { camera_ptr.assume_init() };
        let camera = NonNull::new(camera_ptr).unwrap();
       
        
        // choose which camera to read from
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
        
        
        
        let camera_outputs = unsafe { camera.as_ref().output };
        
        
        let still_port_ptr = unsafe { *(camera_outputs.offset(MMAL_CAMERA_CAPTURE_PORT) as *mut *mut ffi::MMAL_PORT_T) };
        let mut still_port = unsafe { *still_port_ptr };
        let mut format = unsafe { *(still_port.format) };
        
        // https://github.com/raspberrypi/userland/blob/master/host_applications/linux/apps/raspicam/RaspiStillYUV.c#L799
        
        //if self.use_encoder {
            format.encoding = MMAL_ENCODING_OPAQUE;
            /*
        } else {
            (*format).encoding = encoding;
            (*format).encoding_variant = 0; //Irrelevant when not in opaque mode
        }
        */
        
        // es = elementary stream
        let mut es = unsafe { *(format.es) };
        
        es.video.width = w & !(32-1); // VCOS_ALIGN_UP ffi::vcos_align_up(w, 32);
        es.video.height = (h + 16 - 1) & !(16-1); // ffi::vcos_align_up(h, 16);
        es.video.crop.x = 0;
        es.video.crop.y = 0;
        es.video.crop.width = w as i32;
        es.video.crop.height = h as i32;
        es.video.frame_rate.num = 0; //STILLS_FRAME_RATE_NUM;
        es.video.frame_rate.den = 1; //STILLS_FRAME_RATE_DEN;
        
        let status = unsafe { ffi::mmal_port_format_commit(still_port_ptr) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to commit still port configuration".to_string()
            })
        }


        // raspistill sets buffer_num
        if still_port.buffer_num < 3 {
            still_port.buffer_num = 3 as u32;
        }


        // enables camera component
        let status = unsafe { ffi::mmal_component_enable(camera.as_ptr()) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to enable camera component".to_string()
            })
        }
        // END CAMERA STUFF ... KINDA



        // BEGIN ENCODER STUFF
        
        let mut encoder_ptr = MaybeUninit::<*mut ffi::MMAL_COMPONENT_T>::uninit();
        let component: *const c_char = ffi::MMAL_COMPONENT_DEFAULT_IMAGE_ENCODER.as_ptr() as *const c_char;
        let status = unsafe { ffi::mmal_component_create(component, encoder_ptr.as_mut_ptr()) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Failed to create encoder".to_string()
            })
        }
        let encoder_ptr: *mut ffi::MMAL_COMPONENT_T = unsafe { encoder_ptr.assume_init() };
        let encoder = NonNull::new(encoder_ptr).unwrap();

        let encoder_ref = unsafe { encoder.as_ref() };
        if encoder_ref.input_num == 0 || encoder_ref.output_num == 0 {
            return Err(CameraError {
                code: 1,
                message: "Encoder component doesnt have input/output ports".to_string()
            })
        }

        // TODO: input and output are technically arrays in C land ... 
        // though since we're only concerned with the first element, we may not need to dereference
        let encoder_input_rmut_rmut: *mut *mut ffi::MMAL_PORT_T = encoder_ref.input;
        let encoder_output: *mut *mut ffi::MMAL_PORT_T = encoder_ref.output;

        let encoder_input_rmut = unsafe { *encoder_input_rmut_rmut };
        let encoder_input = unsafe { *encoder_input_rmut };
        let mut encoder_output = unsafe { *(*encoder_output) };


        unsafe {
            ffi::mmal_format_copy(encoder_output.format, encoder_input.format);
        }

        // Specify out output format
        unsafe { (*(encoder_output.format)).encoding = MMAL_ENCODING_JPEG };

        encoder_output.buffer_size = encoder_output.buffer_size_recommended;

        if encoder_output.buffer_size < encoder_output.buffer_size_min {
            encoder_output.buffer_size = encoder_output.buffer_size_min;
        }

        encoder_output.buffer_num = encoder_output.buffer_num_recommended;

        if encoder_output.buffer_num < encoder_output.buffer_num_min {
            encoder_output.buffer_num = encoder_output.buffer_num_min;
        }

        // Commit the port changes to the output port
        let status = unsafe { ffi::mmal_port_format_commit( *(encoder_ref.output) ) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to set format on video encoder output port".to_string()
            })
        }

        // Set the JPEG quality level
        let status = unsafe { ffi::mmal_port_parameter_set_uint32( *(encoder_ref.output), ffi::MMAL_PARAMETER_JPEG_Q_FACTOR, 100) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to set JPEG quality on video encoder output port".to_string()
            })
        }


        let status = unsafe { ffi::mmal_port_parameter_set_uint32( *(encoder_ref.output), ffi::MMAL_PARAMETER_JPEG_RESTART_INTERVAL, 0) };
        // NOTE: i think status will bomb if we're setting interval to 0 ... dunno
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to set JPEG restart interval on video encoder output port".to_string()
            })
        }

        // Enable encoder component
        let status = unsafe { ffi::mmal_component_enable( encoder.as_ptr() ) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Unable to enable video encoder component".to_string()
            })
        }


        /* Create pool of buffer headers for the output port to consume */
        let pool = unsafe { ffi::mmal_port_pool_create( *(encoder_ref.output), (*(*encoder_ref.output)).buffer_num, (*(*encoder_ref.output)).buffer_size) };
        
        // not sure how to check whether we got a null pointer back
        /*
        if pool ==  {
            return Err(CameraError {
                code: 1,
                message: "Failed to create buffer header pool for encoder output port".to_string()
            })
        }
        */

        //state->encoder_pool = pool;
        //state->encoder_component = encoder;


        // END ENCODER STUFF


        // Now connect camera capture port to the encoder input
        let mut connection_ptr = MaybeUninit::<*mut ffi::MMAL_CONNECTION_T>::uninit();
        let status = unsafe {
            ffi::mmal_connection_create(connection_ptr.as_mut_ptr(), still_port_ptr, encoder_input_rmut, ffi::MMAL_CONNECTION_FLAG_TUNNELLING | ffi::MMAL_CONNECTION_FLAG_ALLOCATION_ON_INPUT) };
        if status != ffi::MMAL_STATUS_T_MMAL_SUCCESS {
            return Err(CameraError {
                code: 1,
                message: "Failed to connect capture port to encoder input".to_string()
            })
        }
        let connection_ptr: *mut ffi::MMAL_CONNECTION_T = unsafe { connection_ptr.assume_init() };
        let connection = NonNull::new(connection_ptr).unwrap();

        let connection_ref = unsafe { connection.as_ref() };

        let status = unsafe { ffi::mmal_connection_enable(connection.as_ptr()) };
        if (status != ffi::MMAL_STATUS_T_MMAL_SUCCESS) {
            unsafe { ffi::mmal_connection_destroy(connection_ptr) };

            return Err(CameraError {
                code: 1,
                message: "Failed to enable connection".to_string()
            })
        }


        // COPIED FROM C CODE, LEFT OFF HERE

        // TODO: these semaphore functions aren't ending up in our rust bindings
        // Semaphore stuff to synchronize IO operations
        // semaphore which is posted when we reach end of frame (indicates end of capture or fault)
        let mut semaphore_ptr = MaybeUninit::<*mut ffi::MMAL_CONNECTION_T>::uninit();
        let status = unsafe { ffi::vcos_semaphore_create(semaphore_ptr.as_mut_ptr(), "RustSec-sem", 0) };
        if (status != ffi::MMAL_STATUS_T_MMAL_SUCCESS) {
            // TODO: tear down everything?
            return Err(CameraError {
                code: 1,
                message: "Failed to create semaphore".to_string()
            })
        }

        let running = true;
        while running == true {
            // simple timeout for a single capture
            // TODO: fix
            // this just uses nanosleep
            vcos_sleep(state->timeout);

            // TODO: not sure which one we need
            if (state.datetime)
            {
                time_t rawtime;
                struct tm *timeinfo;

                time(&rawtime);
                timeinfo = localtime(&rawtime);

                frame = timeinfo->tm_mon+1;
                frame *= 100;
                frame += timeinfo->tm_mday;
                frame *= 100;
                frame += timeinfo->tm_hour;
                frame *= 100;
                frame += timeinfo->tm_min;
                frame *= 100;
                frame += timeinfo->tm_sec;
            }
            if (state.timestamp)
            {
                frame = (int)time(NULL);
            }



            int num, q;

            // Must do this before the encoder output port is enabled since
            // once enabled no further exif data is accepted
            if ( state.enableExifTags )
            {
                struct gps_data_t *gps_data = raspi_gps_lock();
                add_exif_tags(&state, gps_data);
                raspi_gps_unlock();
            }
            else
            {
                mmal_port_parameter_set_boolean(
                state.encoder_component->output[0], MMAL_PARAMETER_EXIF_DISABLE, 1);
            }

            // Same with raw, apparently need to set it for each capture, whilst port
            // is not enabled
            if (state.wantRAW)
            {
                if (mmal_port_parameter_set_boolean(camera_still_port, MMAL_PARAMETER_ENABLE_RAW_CAPTURE, 1) != MMAL_SUCCESS)
                {
                vcos_log_error("RAW was requested, but failed to enable");
                }
            }

            // There is a possibility that shutter needs to be set each loop.
            if (mmal_status_to_int(mmal_port_parameter_set_uint32(state.camera_component->control, MMAL_PARAMETER_SHUTTER_SPEED, state.camera_parameters.shutter_speed)) != MMAL_SUCCESS)
                vcos_log_error("Unable to set shutter speed");

            // Enable the encoder output port
            encoder_output_port->userdata = (struct MMAL_PORT_USERDATA_T *)&callback_data;

            if (state.common_settings.verbose)
                fprintf(stderr, "Enabling encoder output port\n");

            // Enable the encoder output port and tell it its callback function
            status = mmal_port_enable(encoder_output_port, encoder_buffer_callback);

            // Send all the buffers to the encoder output port
            num = mmal_queue_length(state.encoder_pool->queue);

            for (q=0; q<num; q++)
            {
                MMAL_BUFFER_HEADER_T *buffer = mmal_queue_get(state.encoder_pool->queue);

                if (!buffer)
                vcos_log_error("Unable to get a required buffer %d from pool queue", q);

                if (mmal_port_send_buffer(encoder_output_port, buffer)!= MMAL_SUCCESS)
                vcos_log_error("Unable to send a buffer to encoder output port (%d)", q);
            }

            if (state.burstCaptureMode)
            {
                mmal_port_parameter_set_boolean(state.camera_component->control,  MMAL_PARAMETER_CAMERA_BURST_CAPTURE, 1);
            }

            if(state.camera_parameters.enable_annotate)
            {
                if ((state.camera_parameters.enable_annotate & ANNOTATE_APP_TEXT) && state.common_settings.gps)
                {
                char *text = raspi_gps_location_string();
                raspicamcontrol_set_annotate(state.camera_component, state.camera_parameters.enable_annotate,
                                                text,
                                                state.camera_parameters.annotate_text_size,
                                                state.camera_parameters.annotate_text_colour,
                                                state.camera_parameters.annotate_bg_colour,
                                                state.camera_parameters.annotate_justify,
                                                state.camera_parameters.annotate_x,
                                                state.camera_parameters.annotate_y
                                            );
                free(text);
                }
                else
                raspicamcontrol_set_annotate(state.camera_component, state.camera_parameters.enable_annotate,
                                                state.camera_parameters.annotate_string,
                                                state.camera_parameters.annotate_text_size,
                                                state.camera_parameters.annotate_text_colour,
                                                state.camera_parameters.annotate_bg_colour,
                                                state.camera_parameters.annotate_justify,
                                                state.camera_parameters.annotate_x,
                                                state.camera_parameters.annotate_y
                                            );
            }

            if (state.common_settings.verbose)
                fprintf(stderr, "Starting capture %d\n", frame);

            if (mmal_port_parameter_set_boolean(camera_still_port, MMAL_PARAMETER_CAPTURE, 1) != MMAL_SUCCESS)
            {
                vcos_log_error("%s: Failed to start capture", __func__);
            }
            else
            {
                // Wait for capture to complete
                // For some reason using vcos_semaphore_wait_timeout sometimes returns immediately with bad parameter error
                // even though it appears to be all correct, so reverting to untimed one until figure out why its erratic
                vcos_semaphore_wait(&callback_data.complete_semaphore);
                if (state.common_settings.verbose)
                fprintf(stderr, "Finished capture %d\n", frame);
            }

            // Ensure we don't die if get callback with no open file
            callback_data.file_handle = NULL;

            if (output_file != stdout)
            {
                rename_file(&state, output_file, final_filename, use_filename, frame);
            }
            else
            {
                fflush(output_file);
            }
            // Disable encoder output port
            status = mmal_port_disable(encoder_output_port);

        }
        vcos_semaphore_delete(&callback_data.complete_semaphore);
        


        
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
