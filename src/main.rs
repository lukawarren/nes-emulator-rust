mod cpu;
mod memory;
mod opcodes;
mod ppu;
mod palette_table;

use cpu::Cpu;
use memory::Memory;
use ppu::Ppu;
use ppu::SCREEN_WIDTH;
use ppu::SCREEN_HEIGHT;
use ppu::CYCLES_PER_FRAME;
use ppu::PATTERN_TABLE_SIZE;
use opcodes::INSTRUCTIONS;
use opcodes::Instruction;

use imgui::{Condition, im_str, Image, StyleVar, TextureId, Window, Context};
use imgui_sdl2::ImguiSdl2;
use sdl2::keyboard::{Keycode, Scancode};
use sdl2::event::Event;

use std::os::raw::c_void;
use std::ops::RangeInclusive;
use imgui_opengl_renderer::Renderer;
use sdl2::EventPump;

const WINDOW_WIDTH: u32 = 961;
const WINDOW_HEIGHT: u32 = 684;
const SCREEN_SCALE: usize = 2;

fn main()
{
    // Init SDL
    let sdl_context = sdl2::init().unwrap();
    let video = sdl_context.video().unwrap();

    // Configure OpenGL
    let gl_attr = video.gl_attr();
    gl_attr.set_context_profile(sdl2::video::GLProfile::Core);
    gl_attr.set_context_version(3, 0);

    // Create window
    let window = video.window("NES", WINDOW_WIDTH, WINDOW_HEIGHT)
        .position_centered()
        .opengl()
        .allow_highdpi()
        .build()
        .unwrap();

    // Init OpenGL
    let _gl_context = window.gl_create_context().unwrap();
    gl::load_with(|s| video.gl_get_proc_address(s) as _);

    // Init ImGui; disable .ini config
    let mut imgui = imgui::Context::create();
    imgui.set_ini_filename(None);

    // ImGui backend
    let mut imgui_sdl2 = imgui_sdl2::ImguiSdl2::new(&mut imgui, &window);
    let renderer = imgui_opengl_renderer::Renderer::new(&mut imgui, |s| video.gl_get_proc_address(s) as _);

    // Init emulation
    let mut ppu = Ppu::default();
    let mut memory = Memory::default();
    let mut cpu = Cpu::from_memory(&mut ppu, &mut memory);

    // Saved states
    let mut saved_cpu = cpu;
    let mut saved_ppu = ppu;
    let mut saved_memory = memory.clone();

    // Create OpenGL textures
    let mut output_texture: u32 = 0;
    let mut pattern_table_textures = [0u32; 2];
    let mut palette = 0;

    unsafe
    {
        gl::GenTextures(1, &mut output_texture);
        gl::BindTexture(gl::TEXTURE_2D, output_texture);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGB as i32, SCREEN_WIDTH as i32, SCREEN_HEIGHT as i32, 0, gl::RGB, gl::UNSIGNED_BYTE, ppu.output.as_ptr() as *const c_void);

        for i in 0..pattern_table_textures.len()
        {
            gl::GenTextures(1, &mut pattern_table_textures[i]);
            gl::BindTexture(gl::TEXTURE_2D, pattern_table_textures[i]);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexImage2D(gl::TEXTURE_2D, 0, gl::RGB as i32, PATTERN_TABLE_SIZE as i32, PATTERN_TABLE_SIZE as i32, 0, gl::RGB, gl::UNSIGNED_BYTE, ppu.get_pattern_table(&mut memory, i as u8, palette).as_ptr() as *const c_void);
        }
    }

    // Begin event loop
    let mut event_pump = sdl_context.event_pump().unwrap();
    'running: loop
    {
        // Poll window events
        for event in event_pump.poll_iter()
        {
            // Defer to ImGui first
            imgui_sdl2.handle_event(&mut imgui, &event);
            if imgui_sdl2.ignore_event(&event) { continue }

            match event
            {
                Event::Quit { .. } | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => break 'running,
                _ => {}
            }
        }

        // Set controller
        memory.controller[0] = 0;
        memory.controller[0] |= if event_pump.keyboard_state().is_scancode_pressed(Scancode::X) { 0x80 } else { 0 };
        memory.controller[0] |= if event_pump.keyboard_state().is_scancode_pressed(Scancode::Z) { 0x40 } else { 0 };
        memory.controller[0] |= if event_pump.keyboard_state().is_scancode_pressed(Scancode::A) { 0x20 } else { 0 };
        memory.controller[0] |= if event_pump.keyboard_state().is_scancode_pressed(Scancode::S) { 0x10 } else { 0 };
        memory.controller[0] |= if event_pump.keyboard_state().is_scancode_pressed(Scancode::Up) { 0x08 } else { 0 };
        memory.controller[0] |= if event_pump.keyboard_state().is_scancode_pressed(Scancode::Down) { 0x04 } else { 0 };
        memory.controller[0] |= if event_pump.keyboard_state().is_scancode_pressed(Scancode::Left) { 0x02 } else { 0 };
        memory.controller[0] |= if event_pump.keyboard_state().is_scancode_pressed(Scancode::Right) { 0x01 } else { 0 };

        // Perform emulation
        on_emulation_cycle(&mut cpu, &mut ppu, &mut memory);

        // Draw ImGUI stuff
        draw_gui
        (
            // Emulation
            &mut cpu,
            &mut ppu,
            &mut memory,

            // Saved states
            &mut saved_cpu,
            &mut saved_ppu,
            &mut saved_memory,

            // Input and output
            output_texture,
            &pattern_table_textures,
            &mut palette,

            // Rendering
            &mut imgui,
            &mut imgui_sdl2,
            &renderer,
            &window,
            &mut event_pump
        );

        window.gl_swap_window();
    }

    // Clean up OpenGL
    unsafe
    {
        gl::DeleteTextures(1, &mut output_texture);

        for i in 0..pattern_table_textures.len()
        {
            gl::DeleteTextures(1, &mut pattern_table_textures[i]);
        }
    }
}

fn on_emulation_cycle(cpu: &mut Cpu, ppu: &mut Ppu, memory: &mut Memory)
{
    for i in 0..CYCLES_PER_FRAME
    {
        // PPU runs at, well... "PPU speed"
        ppu.execute(memory);

        // CPU runs at one third of the speed
        if i % 3 == 0
        {
            // If DMA is happening, execution is temporarily halted
            if memory.dma_happening
            {
                // The DMA circuitry is synced to the CPU clock only every two intervals, so we may need to wait
                if memory.dma_waiting_for_sync
                {
                    if i % 2 == 1
                    {
                        memory.dma_waiting_for_sync = false;
                    }
                }
                else
                {
                    // On even cycles, data is read
                    if i % 2 == 0
                    {
                        memory.dma_data = memory.read_byte(ppu, (memory.dma_page as u16) << 8 | memory.dma_address as u16, false);
                    }

                    // On odd cycles, data is written
                    else
                    {
                        ppu.object_attribute_memory[memory.dma_address as usize] = memory.dma_data;
                        memory.dma_address = memory.dma_address.wrapping_add(1);

                        // If we've looped back round to zero, we've written a full page, so stop (TODO: fix as per the DMA "todo" in memory.rs)
                        if memory.dma_address == 0
                        {
                            memory.dma_happening = false;
                            memory.dma_waiting_for_sync = true;
                        }
                    }
                }
            }
            else
            {
                if cpu.cycles == 0 { cpu.execute(ppu, memory); }
                cpu.cycles -= 1;
            }
        }

        if ppu.due_non_maskable_interrupt
        {
            ppu.due_non_maskable_interrupt = false;
            cpu.on_non_maskable_interrupt(ppu, memory);
        }
    }
}

fn draw_gui
(
    // Emulation
    cpu: &mut Cpu,
    ppu: &mut Ppu,
    memory: &mut Memory,

    // Save states
    saved_cpu: &mut Cpu,
    saved_ppu: &mut Ppu,
    saved_memory: &mut Memory,

    // Input and output
    output_texture: u32,
    pattern_table_textures: &[u32; 2],
    palette: &mut u8,

    // Rendering
    imgui: &mut Context,
    imgui_sdl2: &mut ImguiSdl2,
    renderer: &Renderer,
    window: &sdl2::video::Window,
    event_pump: &mut EventPump
)
{
    // Prepare ImGui
    imgui_sdl2.prepare_frame(imgui.io_mut(), window, &event_pump.mouse_state());

    // Clear screen and update textures
    unsafe
    {
        gl::ClearColor(0.0, 0.0, 0.0, 1.0);
        gl::Clear(gl::COLOR_BUFFER_BIT);

        gl::BindTexture(gl::TEXTURE_2D, output_texture);
        gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, SCREEN_WIDTH as i32, SCREEN_HEIGHT as i32, gl::RGB, gl::UNSIGNED_BYTE, ppu.output.as_ptr() as *const c_void);

        for i in 0..pattern_table_textures.len()
        {
            gl::BindTexture(gl::TEXTURE_2D, pattern_table_textures[i]);
            gl::TexSubImage2D(gl::TEXTURE_2D, 0, 0, 0, PATTERN_TABLE_SIZE as i32, PATTERN_TABLE_SIZE as i32, gl::RGB, gl::UNSIGNED_BYTE, ppu.get_pattern_table(memory, i as u8, *palette).as_ptr() as *const c_void);
        }
    }

    // Begin ImGui
    let ui = imgui.frame();
    let border_size = 1.0;
    let border = ui.push_style_var(StyleVar::WindowBorderSize(border_size));
    let margin = 5.0;
    let bar_height = 18.0;

    // Output window
    let padding = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
    let output_x = margin;
    let output_y = margin;
    let output_width = (SCREEN_WIDTH*SCREEN_SCALE) as f32;
    let output_height = (SCREEN_HEIGHT*SCREEN_SCALE) as f32;

    Window::new(im_str!("Output"))
        .position([output_x, output_y], Condition::Always)
        .resizable(false)
        .build(&ui, ||
        {
            Image::new(TextureId::from(output_texture as usize), [output_width, output_height]).build(&ui);
        });

    padding.pop(&ui);

    // Registers
    let cpu_section_width = 700;
    let registers_x = output_x + output_width + border_size + margin - 1.0;
    let registers_width = cpu_section_width as f32 - registers_x - margin;
    let registers_height = 140.0;

    Window::new(im_str!("Registers"))
        .position([registers_x, output_y], Condition::Always)
        .size([registers_width, registers_height], Condition::Always)
        .resizable(false)
        .build(&ui, ||
        {
            ui.text(format!("Flags: {:#04b}", cpu.flags.bits()));
            ui.text(format!("PC: {:#06x}", cpu.pc));
            ui.text(format!("SP: {:#04x}", cpu.sp));
            ui.text(format!("A: {:#04x}", cpu.a));
            ui.text(format!("X: {:#04x}", cpu.x));
            ui.text(format!("Y: {:#04x}", cpu.y));
        });

    // Stack
    Window::new(im_str!("Stack"))
        .position([output_x, output_y + bar_height + output_height + border_size + margin], Condition::Always)
        .size([output_width + margin + registers_width, 170.0], Condition::Always)
        .resizable(false)
        .build(&ui, ||
        {

            // 256 bytes in the stack, 16x16 --> 32x8
            let rows: u16 = 8;
            for row in 0..rows
            {
                let mut bytes = [0u8; 32];

                for i in 0..bytes.len()
                {
                    bytes[i] = memory.read_byte(ppu, row * rows as u16 + i as u16, true);
                }

                ui.text_colored([0.3, 0.3, 0.3, 1.0], format!(
                    "{:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x} {:#04x}",
                    bytes[0], bytes[1], bytes[2], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7], bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15])
                );
            }
        });

    // Disassembly
    Window::new(im_str!("Disassembly"))
        .position([registers_x, output_y + registers_height + margin], Condition::Always)
        .size([registers_width, output_height + bar_height - registers_height - margin + border_size], Condition::Always)
        .resizable(false)
        .build(&ui, ||
        {

            let old_pc = cpu.pc;

            for row in 0..32u16
            {
                // The bellow code with affect the program counter *on purpose*
                let current_pc = cpu.pc;

                // Fetch opcode
                let opcode = memory.read_byte(ppu, cpu.pc, true);
                let Instruction(name, _, addressing_mode, _) = &INSTRUCTIONS[opcode as usize];
                cpu.pc += 1;

                // Fetch operand
                let operand = cpu.fetch_operand(ppu, memory, addressing_mode, true);

                // Display
                let colour = if row == 0 { [1.0, 1.0, 1.0, 1.0] } else { [0.3, 0.3, 0.3, 1.0] };
                ui.text_colored(colour, format!("{:#06x} {} {:#06x}", current_pc, name, operand.data))
            }

            cpu.pc = old_pc;
        });

    // Pattern tables
    let pattern_table_padding = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
    let pattern_table_size = (PATTERN_TABLE_SIZE * SCREEN_SCALE) as f32;
    let pattern_table_x = cpu_section_width as f32;

    Window::new(im_str!("Pattern table zero"))
        .position([pattern_table_x, output_y], Condition::Always)
        .resizable(false)
        .build(&ui, ||
        {
            Image::new(TextureId::from(pattern_table_textures[0] as usize), [pattern_table_size, pattern_table_size]).build(&ui);
        });

    let pattern_table_window_height = bar_height + pattern_table_size + border_size + margin;

    Window::new(im_str!("Pattern table one"))
        .position([pattern_table_x, output_y + pattern_table_window_height], Condition::Always)
        .resizable(false)
        .build(&ui, ||
        {
            Image::new(TextureId::from(pattern_table_textures[1] as usize), [pattern_table_size, pattern_table_size]).build(&ui);
        });

    pattern_table_padding.pop(&ui);

    // Misc menu
    Window::new(im_str!("Miscellaneous"))
        .position([pattern_table_x, output_y + pattern_table_window_height*2.0], Condition::Always)
        .size([pattern_table_size, WINDOW_HEIGHT as f32 - pattern_table_window_height*2.0 - margin*2.0], Condition::Always)
        .resizable(false)
        .build(&ui, ||
        {
            imgui::Slider::new(im_str!("Palette")).range(RangeInclusive::new(0, 7))
                .build(&ui, palette);

            ui.button(im_str!("Save emulation state"), [150.0, 20.0]).then(||
            {
                *saved_cpu = *cpu;
                *saved_ppu = *ppu;
                *saved_memory = memory.clone();
            });

            ui.button(im_str!("Load emulation state"), [150.0, 20.0]).then(||
                {
                *cpu = *saved_cpu;
                *ppu = *saved_ppu;
                *memory = saved_memory.clone();
            });
        });

    border.pop(&ui);

    // Render ImGui
    imgui_sdl2.prepare_render(&ui, &window);
    renderer.render(ui);
}