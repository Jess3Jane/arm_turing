extern crate unicorn;

use std::fs::File;
use std::io::prelude::*;
use std::process::Command;
use unicorn::{Cpu, CpuARM};

fn exec(
    code: &Vec<u8>,
    start_addr: u64,
    data_addr: usize,
    data_len: usize,
    exec_len: usize,
) -> Result<CpuARM, unicorn::Error> {
    let mut emu = CpuARM::new(unicorn::Mode::LITTLE_ENDIAN)?;
    emu.mem_map(0, (code.len() / 0x1000 + 1) * 0x1000, unicorn::PROT_ALL);
    emu.mem_write(0, code);
    emu.reg_write(unicorn::arm_const::RegisterARM::R8, 0x4000);
    emu.add_code_hook(
        unicorn::unicorn_const::CodeHookType::CODE,
        start_addr,
        start_addr,
        move |emu, _, _| {
            print_data(&emu.mem_read(data_addr as u64 + 1, data_len).unwrap());
        },
    );
    emu.emu_start(
        start_addr,
        start_addr + code.len() as u64,
        10 * unicorn::SECOND_SCALE,
        exec_len,
    );
    Ok(emu)
}

fn load_and_exec(file: &str, data_addr: usize, data_len: usize) {
    let mut arm_code = Vec::new();
    File::open(file).unwrap().read_to_end(&mut arm_code);

    let emu = exec(&arm_code, 0x1000, data_addr, data_len, 60000).unwrap();
}

fn preprocess(file: &str, output: &str) {
    let mut code = String::new();
    let mut expanded = String::new();
    File::open(file).unwrap().read_to_string(&mut code);

    for line in code.split("\n") {
        expanded.push_str(&match line.starts_with("!") {
            false => String::from(line),
            true => match line {
                "!incr" => format!(
                    "incr:\n.byte {}",
                    (0..256)
                        .map(|i| format!("{}", (i + 1) % 256))
                        .collect::<Vec<String>>()
                        .join(",")
                ),
                "!add" => format!(
                    "add:\n.byte {}",
                    (0..256)
                        .map(|a| (0..256)
                            .map(|b| format!("{}", (a + b) % 256))
                            .collect::<Vec<String>>()
                            .join(","))
                        .collect::<Vec<String>>()
                        .join(",")
                ),
                "!literal" => format!(
                    "literal:\n.byte {}",
                    (0..256)
                        .map(|a| format!("{}", a))
                        .collect::<Vec<String>>()
                        .join(",")
                ),
                line => panic!("Unsupported preprocessor directive {}", line),
            },
        });
        expanded.push('\n');
    }

    File::create(output)
        .unwrap()
        .write_fmt(format_args!("{}", expanded));
}

fn assemble(data: &Vec<u8>, output: &str) -> usize {
    let mut process_rule = String::new();
    File::open("templates/process_rule.s")
        .unwrap()
        .read_to_string(&mut process_rule);
    let mut copy = String::new();
    File::open("templates/copy.s")
        .unwrap()
        .read_to_string(&mut copy);
    let mut rule = String::new();
    File::open("templates/rule.s")
        .unwrap()
        .read_to_string(&mut rule);
    let mut code = File::create(output).unwrap();

    // literal lut
    code.write_all(b"!literal\n");

    // top of the loop
    code.write_all(b".org 0x1000\n");
    code.write_all(b"_loop:\n");

    // enough rule processors to process the whole vector
    for i in 0..data.len() {
        code.write_all(
            &process_rule
                .replace("!location2", &format!("#tmp + {}", i + 1))
                .replace("!location", &format!("#data + {}", i + 1))
                .replace("!rule", "#rule")
                .as_str()
                .as_bytes(),
        );
        code.write_all(b"\n");
    }

    // enough copiers to copy the whole vector
    for i in 0..data.len() {
        code.write_all(
            &copy
                .replace("!from", &format!("#tmp + {}", i + 1))
                .replace("!to", &format!("#data + {}", i + 1))
                .as_str()
                .as_bytes(),
        );
        code.write_all(b"\n");
    }

    // loop
    code.write_all(b"mov pc, #_loop\n");

    // rule lut
    code.write_all(b"rule:\n");
    code.write_all(rule.as_str().as_bytes());
    code.write_all(b"\n\n");

    let data_addr = data.len() * 0x100 + 0x1000;
    code.write_all(format!(".org {}\n", data_addr).as_str().as_bytes());
    // data vector
    code.write_all(b"data:\n.byte ");
    code.write_all(b"0, ");
    code.write_all(
        data.iter()
            .map(|i| format!("{}", i))
            .collect::<Vec<String>>()
            .join(",")
            .as_str()
            .as_bytes(),
    );
    code.write_all(b", 0");
    code.write_all(b"\n\n");

    // tmp vector
    code.write_all(b"tmp:\n.byte ");
    code.write_all(
        (0..data.len() + 2)
            .map(|_| String::from("0"))
            .collect::<Vec<String>>()
            .join(",")
            .as_str()
            .as_bytes(),
    );
    code.write_all(b"\n\n");

    data_addr
}

fn print_data(data: &[u8]) {
    println!(
        "|{}|",
        data.iter()
            .map(|v| match v {
                0 => ' ',
                1 => '#',
                _ => '~',
            })
            .collect::<String>()
    );
}

fn main() {
    let mut data = vec![0; 64];
    data[63] = 1;
    let data_addr = assemble(&data, "assembled_code.s");
    preprocess("assembled_code.s", "preprocessed_code.s");

    let ret = Command::new("arm-none-eabi-as")
        .arg("preprocessed_code.s")
        .arg("-o")
        .arg("test.o")
        .output()
        .unwrap();
    if !ret.status.success() {
        println!("{}", String::from_utf8_lossy(&ret.stderr));
        return;
    }

    let ret = Command::new("arm-none-eabi-objcopy")
        .arg("-O")
        .arg("binary")
        .arg("test.o")
        .arg("binfile")
        .output()
        .unwrap();
    if !ret.status.success() {
        println!("{}", String::from_utf8_lossy(&ret.stderr));
        return;
    }

    load_and_exec("binfile", data_addr, data.len());
}
