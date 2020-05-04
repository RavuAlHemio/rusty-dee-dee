use std::convert::TryInto;
use std::error::Error;
use std::fmt::{Display, Error as FmtError, Formatter};
use std::fs::File;
use std::io::{Error as IOError, ErrorKind as IOErrorKind};
use std::mem::{size_of, zeroed};
use std::os::windows::io::{AsRawHandle, RawHandle};
use std::ptr::{null_mut, read as ptr_read};
use std::string::FromUtf16Error;

use winapi::STRUCT;
use winapi::shared::minwindef::{BOOL, DWORD, ULONG};
use winapi::shared::ntdef::{BOOLEAN, HANDLE, NTSTATUS, PHANDLE, PULONG, PUNICODE_STRING, PVOID};
use winapi::shared::ntdef::{PWSTR, UNICODE_STRING};
use winapi::shared::ntstatus::{STATUS_NO_MORE_ENTRIES, STATUS_SUCCESS};
use winapi::shared::winerror::HRESULT_FROM_NT;
use winapi::um::ioapiset::DeviceIoControl;
use winapi::um::winioctl::{GET_LENGTH_INFORMATION, IOCTL_DISK_GET_LENGTH_INFO};
use winapi::um::winnt::ACCESS_MASK;


STRUCT! {
    #[allow(non_snake_case)]
    struct OBJECT_ATTRIBUTES {
        Length: ULONG,
        RootDirectory: HANDLE,
        ObjectName: PUNICODE_STRING,
        Attributes: ULONG,
        SecurityDescriptor: PVOID,
        SecurityQualityOfService: PVOID,
    }
}

STRUCT! {
    #[allow(non_snake_case)]
    struct OBJECT_DIRECTORY_INFORMATION {
        Name: UNICODE_STRING,
        TypeName: UNICODE_STRING,
    }
}

#[link(name = "ntdll")]
extern "system" {
    fn NtOpenDirectoryObject(
        DirectoryHandle: PHANDLE,
        DesiredAccess: ACCESS_MASK,
        ObjectAttributes: *mut OBJECT_ATTRIBUTES,
    ) -> NTSTATUS;

    fn NtQueryDirectoryObject(
        DirectoryHandle: PHANDLE,
        Buffer: PVOID,
        Length: ULONG,
        ReturnSingleEntry: BOOLEAN,
        RestartScan: BOOLEAN,
        Context: PULONG,
        ReturnLength: PULONG,
    ) -> NTSTATUS;

    fn NtClose(
        Handle: HANDLE,
    ) -> NTSTATUS;
}

const DIRECTORY_QUERY: ACCESS_MASK = 0x0001;


#[derive(Debug)]
struct UnicodeStringSizeOverflow {}
impl Display for UnicodeStringSizeOverflow {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> Result<(), FmtError> {
        write!(formatter, "UNICODE_STRING size overflow")
    }
}
impl Error for UnicodeStringSizeOverflow {
}

struct UnicodeStringHolder {
    backing: Vec<u16>,
}
impl UnicodeStringHolder {
    pub fn new_from_vec(vec: Vec<u16>) -> Result<UnicodeStringHolder, UnicodeStringSizeOverflow> {
        let byte_capacity = vec.capacity() * size_of::<u16>();
        if byte_capacity > std::u16::MAX.into() {
            Err(UnicodeStringSizeOverflow {})
        } else {
            Ok(UnicodeStringHolder {
                backing: vec,
            })
        }
    }

    pub fn new_buffer(char_capacity: usize) -> Result<UnicodeStringHolder, UnicodeStringSizeOverflow> {
        let backing: Vec<u16> = Vec::with_capacity(char_capacity);
        UnicodeStringHolder::new_from_vec(backing)
    }

    pub fn new_from_string(s: &str) -> Result<UnicodeStringHolder, UnicodeStringSizeOverflow> {
        let backing: Vec<u16> = s.encode_utf16().collect();
        UnicodeStringHolder::new_from_vec(backing)
    }

    pub fn new_from_unicode_string(unistr: &UNICODE_STRING) -> UnicodeStringHolder {
        let byte_length: usize = unistr.Length.into();
        let char_length: isize = (byte_length / size_of::<u16>()).try_into().unwrap();
        let mut backing: Vec<u16> = Vec::new();
        unsafe {
            for i in 0..char_length {
                let c: u16 = *(unistr.Buffer.offset(i));
                backing.push(c);
            }
        }
        UnicodeStringHolder {
            backing,
        }
    }

    pub fn as_unicode_string(&mut self) -> UNICODE_STRING {
        let byte_length: u16 = (self.backing.len() * size_of::<u16>()).try_into().unwrap();
        let byte_capacity: u16 = (self.backing.capacity() * size_of::<u16>()).try_into().unwrap();
        let buf: PWSTR = self.backing.as_mut_ptr();
        UNICODE_STRING {
            Length: byte_length,
            MaximumLength: byte_capacity,
            Buffer: buf,
        }
    }

    pub fn try_as_string(&self) -> Result<String, FromUtf16Error> {
        String::from_utf16(&self.backing)
    }
}

struct NtCloseHandle {
    handle: HANDLE,
}
impl Drop for NtCloseHandle {
    fn drop(&mut self) {
        unsafe {
            NtClose(self.handle);
        }
    }
}


fn io_error_from_nt_status(nt_status: NTSTATUS) -> IOError {
    let error_code = HRESULT_FROM_NT(nt_status as u32);
    IOError::from_raw_os_error(error_code)
}


struct DirectoryEntry {
    name: String,
    type_name: String,
}


fn enumerate_object_path(path: &str) -> Result<Vec<DirectoryEntry>, IOError> {
    let mut devices_path_holder = UnicodeStringHolder::new_from_string(path).unwrap();
    let mut devices_path = devices_path_holder.as_unicode_string();
    let mut attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>().try_into().unwrap(),
        RootDirectory: null_mut(),
        ObjectName: &mut devices_path,
        Attributes: 0u32,
        SecurityDescriptor: null_mut(),
        SecurityQualityOfService: null_mut(),
    };

    let mut directory_handle: HANDLE = unsafe { zeroed() };
    let status = unsafe {
        NtOpenDirectoryObject(
            &mut directory_handle,
            DIRECTORY_QUERY,
            &mut attributes,
        )
    };
    if status != STATUS_SUCCESS {
        return Err(io_error_from_nt_status(status));
    }
    let dir_handle = NtCloseHandle { handle: directory_handle };

    let mut buf: [u8; 4096] = unsafe { zeroed() };
    let mut context: ULONG = 0;
    let mut return_length: ULONG = 0;
    const RETURN_SINGLE_ENTRY: BOOLEAN = 1;
    const DONT_RESTART_SCAN: BOOLEAN = 0;
    let mut ret: Vec<DirectoryEntry> = Vec::new();
    loop {
        let status = unsafe {
            NtQueryDirectoryObject(
                dir_handle.handle as PHANDLE,
                buf.as_mut_ptr() as PVOID,
                buf.len().try_into().unwrap(),
                RETURN_SINGLE_ENTRY,
                DONT_RESTART_SCAN,
                &mut context,
                &mut return_length,
            )
        };
        if status == STATUS_NO_MORE_ENTRIES {
            break;
        } else if status != STATUS_SUCCESS {
            return Err(io_error_from_nt_status(status));
        }

        // interpret the struct
        let dir_info: OBJECT_DIRECTORY_INFORMATION = unsafe {
            ptr_read(buf.as_ptr() as *const OBJECT_DIRECTORY_INFORMATION)
        };
        let name_res = UnicodeStringHolder::new_from_unicode_string(&dir_info.Name)
            .try_as_string();
        let type_name_res = UnicodeStringHolder::new_from_unicode_string(&dir_info.TypeName)
            .try_as_string();
        if name_res.is_err() || type_name_res.is_err() {
            continue;
        }

        let entry = DirectoryEntry {
            name: name_res.unwrap(),
            type_name: type_name_res.unwrap(),
        };
        ret.push(entry);
    }

    Ok(ret)
}


pub fn get_windows_disks() -> Result<Vec<String>, IOError> {
    let mut ret: Vec<String> = Vec::new();

    let mut devs = enumerate_object_path("\\Device")?;
    devs.retain(|dev| dev.type_name == "Directory" && dev.name.starts_with("Harddisk"));
    for dev in devs {
        let dev_path = format!("\\Device\\{}", dev.name);

        let mut partitions = enumerate_object_path(&dev_path)?;
        partitions.retain(|pt| pt.name.starts_with("Partition"));
        for partition in partitions {
            let part_path = format!("\\\\?\\GLOBALROOT\\Device\\{}\\{}", dev.name, partition.name);
            ret.push(part_path);
        }
    }

    Ok(ret)
}


pub fn get_disk_size(file: &File) -> Result<u64, IOError> {
    let file_handle: RawHandle = file.as_raw_handle();

    let mut length_info: GET_LENGTH_INFORMATION = unsafe { zeroed() };
    let length_info_size: DWORD = size_of::<GET_LENGTH_INFORMATION>().try_into().unwrap();
    let mut bytes_returned: DWORD = 0;
    let result: BOOL = unsafe {
        DeviceIoControl(
            file_handle as HANDLE,
            IOCTL_DISK_GET_LENGTH_INFO,
            null_mut(),
            0,
            &mut length_info as *mut GET_LENGTH_INFORMATION as PVOID,
            length_info_size,
            &mut bytes_returned,
            null_mut(),
        )
    };
    if result == 0 {
        return Err(IOError::last_os_error());
    }
    let length: i64 = unsafe { *length_info.Length.QuadPart() };
    if length < 0 {
        return Err(IOError::new(
            IOErrorKind::InvalidData,
            format!("length is {}, expected >= 0", length),
        ));
    }

    Ok(length.try_into().unwrap())
}
