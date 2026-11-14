use super::*;

unsafe fn new_machine() -> *mut z180_machine {
    let mut config = mem::MaybeUninit::uninit();
    let mut handle = ptr::null_mut();
    unsafe {
        assert_eq!(
            z180_config_init(config.as_mut_ptr(), ptr::null_mut()),
            Z180_OK
        );
        assert_eq!(
            z180_create(
                config.as_ptr(),
                ptr::null(),
                0,
                ptr::null(),
                &mut handle,
                ptr::null_mut()
            ),
            Z180_OK
        );
    }
    handle
}

#[test]
fn panic_poisons_only_its_machine_and_contains_payload_destructor() {
    struct BadDrop;
    impl Drop for BadDrop {
        fn drop(&mut self) {
            panic!("panic payload drop");
        }
    }
    unsafe {
        let first = new_machine();
        let second = new_machine();
        let mut error = mem::MaybeUninit::<z180_error>::uninit();
        assert_eq!(
            machine(first, error.as_mut_ptr(), |_| std::panic::panic_any(
                BadDrop
            )),
            Z180_INTERNAL_ERROR
        );
        assert_eq!(error.assume_init().status, Z180_INTERNAL_ERROR);
        assert_eq!(z180_reset(first, ptr::null_mut()), Z180_POISONED);
        assert_eq!(z180_reset(second, ptr::null_mut()), Z180_OK);
        z180_destroy(first);
        z180_destroy(second);
    }
}

#[test]
fn invalid_shapes_do_not_create_slices_or_mutate_outputs() {
    unsafe {
        let machine = new_machine();
        let mut value = 123;
        assert_eq!(
            z180_reg_get(machine, u32::MAX, &mut value, ptr::null_mut()),
            Z180_INVALID_ARGUMENT
        );
        assert_eq!(value, 123);
        assert_eq!(
            z180_set_iff1(machine, 2, ptr::null_mut()),
            Z180_INVALID_ARGUMENT
        );
        assert_eq!(
            z180_set_dreq(machine, 2, 1, ptr::null_mut()),
            Z180_INVALID_ARGUMENT
        );
        assert_eq!(
            z180_set_ext_map_table(machine, ptr::null(), 1 << 20, ptr::null_mut()),
            Z180_INVALID_ARGUMENT
        );
        assert!(check::<u64>(ptr::null(), usize::MAX).is_err());
        assert!(check::<u64>(ptr::dangling::<u8>().cast(), 1).is_err());
        assert_eq!(
            z180_load_state(machine, ptr::null(), 0, ptr::null_mut()),
            Z180_INVALID_STATE
        );
        z180_destroy(machine);
    }
}

#[test]
fn save_state_accepts_uninitialized_storage() {
    unsafe {
        let machine = new_machine();
        let mut size = 0;
        assert_eq!(
            z180_save_state_size(machine, &mut size, ptr::null_mut()),
            Z180_OK
        );
        let mut storage = Vec::<mem::MaybeUninit<u8>>::with_capacity(size);
        let mut written = usize::MAX;
        assert_eq!(
            z180_save_state(
                machine,
                storage.as_mut_ptr().cast(),
                1,
                &mut written,
                ptr::null_mut()
            ),
            Z180_BUFFER_TOO_SMALL
        );
        assert_eq!(written, 0);
        assert_eq!(
            z180_save_state(
                machine,
                storage.as_mut_ptr().cast(),
                size,
                &mut written,
                ptr::null_mut()
            ),
            Z180_OK
        );
        assert_eq!(written, size);
        storage.set_len(size);
        let saved: Vec<u8> = storage.iter().map(|b| b.assume_init()).collect();
        assert_eq!(
            z180_load_state(machine, saved.as_ptr(), saved.len(), ptr::null_mut()),
            Z180_OK
        );
        z180_destroy(machine);
    }
}

#[test]
fn diagnostic_is_nul_terminated_utf8_and_success_clears_it() {
    unsafe {
        let mut error = mem::MaybeUninit::<z180_error>::uninit();
        let message = "é".repeat(200);
        assert_eq!(
            boundary(error.as_mut_ptr(), || Err(fail(
                Z180_INVALID_CONFIG,
                message
            ))),
            Z180_INVALID_CONFIG
        );
        let mut error = error.assume_init();
        let bytes: Vec<u8> = error
            .message
            .iter()
            .map(|c| *c as u8)
            .take_while(|c| *c != 0)
            .collect();
        assert_eq!(bytes.len(), 254);
        assert!(std::str::from_utf8(&bytes).is_ok());
        assert_eq!(boundary(&mut error, || Ok(())), Z180_OK);
        assert_eq!(error.status, 0);
        assert_eq!(error.host_code, 0);
        assert!(error.message.iter().all(|b| *b == 0));
    }
}

#[test]
fn all_event_variants_have_explicit_tags_and_zero_unused_fields() {
    let trap = event_record(Event::Trap {
        cycle: 9,
        pc: 4,
        opcode: [0xed, 0, 0],
        len: 2,
    });
    assert_eq!(trap.kind, Z180_EVENT_TRAP);
    assert_eq!(trap.port, 0);
    assert_eq!(trap.physical, 0);
    assert_eq!(trap.opcode, [0xed, 0, 0]);
    let irq = event_record(Event::IrqAck {
        cycle: 12,
        source: IrqSource::Asci1,
        vector: 32,
    });
    assert_eq!(irq.kind, Z180_EVENT_IRQ_ACK);
    assert_eq!(irq.source, Z180_IRQ_ASCI1);
    assert_eq!(irq.pc, 0);
    assert_eq!(irq.opcode, [0; 3]);
    for (event, kind) in [
        (
            Event::IoRead {
                cycle: 1,
                pc: 2,
                port: 3,
                val: 4,
            },
            Z180_EVENT_IO_READ,
        ),
        (
            Event::IoWrite {
                cycle: 1,
                pc: 2,
                port: 3,
                val: 4,
            },
            Z180_EVENT_IO_WRITE,
        ),
        (
            Event::MemRead {
                cycle: 1,
                pc: 2,
                phys: 3,
                val: 4,
            },
            Z180_EVENT_MEM_READ,
        ),
        (
            Event::MemWrite {
                cycle: 1,
                pc: 2,
                phys: 3,
                val: 4,
            },
            Z180_EVENT_MEM_WRITE,
        ),
        (
            Event::RomWrite {
                cycle: 1,
                pc: 2,
                phys: 3,
                val: 4,
            },
            Z180_EVENT_ROM_WRITE,
        ),
    ] {
        let r = event_record(event);
        assert_eq!(r.kind, kind);
        assert_eq!(r.cycle, 1);
        assert_eq!(r.pc, 2);
        assert_eq!(r.value, 4);
        assert_eq!(r.source, 0);
    }
}
