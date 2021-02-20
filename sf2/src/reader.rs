use std::convert::TryInto;

pub struct Reader {
    data: Vec<u8>,
    curr: usize,
}
impl Reader {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data, curr: 0 }
    }

    pub fn read(&mut self, len: usize) -> &[u8] {
        let start = self.curr;
        let end = self.curr + len;
        self.curr = end;

        let out = &self.data[start..end];
        out
    }

    pub fn read_string(&mut self, len: usize) -> String {
        let start = self.curr;
        let end = self.curr + len;
        self.curr = end;

        let data = &self.data[start..end];
        let name = unsafe { std::ffi::CStr::from_ptr(data.as_ptr() as _) };
        let name = name.to_str().unwrap();
        let name = name.to_owned();

        name
    }

    pub fn read_u16(&mut self) -> u16 {
        let start = self.curr;
        let end = self.curr + 2;
        self.curr = end;

        let out: [u8; 2] = self.data[start..end].try_into().unwrap();
        u16::from_be_bytes(out)
    }

    pub fn read_u32(&mut self) -> u32 {
        let start = self.curr;
        let end = self.curr + 4;
        self.curr = end;

        let out: [u8; 4] = self.data[start..end].try_into().unwrap();
        u32::from_be_bytes(out)
    }
}
