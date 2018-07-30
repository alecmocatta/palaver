mod to_hex {
	use std::fmt;
	#[derive(Clone)]
	pub struct Hex<'a>(&'a [u8], bool);
	impl<'a> Iterator for Hex<'a> {
		type Item = char;

		fn next(&mut self) -> Option<char> {
			if !self.0.is_empty() {
				const CHARS: &[u8] = b"0123456789abcdef";
				let byte = self.0[0];
				let second = self.1;
				if second {
					self.0 = self.0.split_first().unwrap().1;
				}
				self.1 = !self.1;
				Some(CHARS[if !second { byte >> 4 } else { byte & 0xf } as usize] as char)
			} else {
				None
			}
		}
	}
	impl<'a> fmt::Display for Hex<'a> {
		fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
			for char_ in self.clone() {
				write!(f, "{}", char_)?;
			}
			Ok(())
		}
	}
	pub trait ToHex {
		fn to_hex(&self) -> Hex; // TODO: make impl Iterator when poss
	}
	impl ToHex for [u8] {
		fn to_hex(&self) -> Hex {
			Hex(&*self, false)
		}
	}
}
pub use self::to_hex::ToHex;
