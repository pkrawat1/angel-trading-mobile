//! Reads Erlang's external term format, as far as `:erlang.term_to_binary/1`
//! uses it for the web app's client records: maps, tuples, lists, atoms,
//! binaries and numbers.

#[derive(Clone, Debug, PartialEq)]
pub enum Term {
    Atom(String),
    Binary(Vec<u8>),
    Int(i64),
    Float(f64),
    List(Vec<Term>),
    Tuple(Vec<Term>),
    Map(Vec<(Term, Term)>),
}

impl Term {
    /// The value under an atom key, as in `map.api_key`.
    pub fn get(&self, key: &str) -> Option<&Term> {
        let Term::Map(pairs) = self else { return None };
        pairs
            .iter()
            .find(|(k, _)| matches!(k, Term::Atom(atom) if atom == key))
            .map(|(_, value)| value)
    }

    /// An Elixir string. `nil` and other atoms are not strings.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Term::Binary(bytes) => std::str::from_utf8(bytes).ok(),
            _ => None,
        }
    }
}

const VERSION: u8 = 131;
// Far deeper than any record the web app writes, and shallow enough that a
// corrupt value cannot exhaust the stack.
const MAX_DEPTH: usize = 32;

/// `None` for anything malformed, truncated, followed by stray bytes, or using
/// a tag the web app never writes (compressed terms, pids, references, funs).
pub fn decode(bytes: &[u8]) -> Option<Term> {
    let (&version, rest) = bytes.split_first()?;
    if version != VERSION {
        return None;
    }
    let mut reader = Reader { bytes: rest };
    let term = reader.term(0)?;
    reader.bytes.is_empty().then_some(term)
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.bytes.len() < n {
            return None;
        }
        let (head, tail) = self.bytes.split_at(n);
        self.bytes = tail;
        Some(head)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn u8(&mut self) -> Option<usize> {
        Some(self.take(1)?[0].into())
    }

    fn u16(&mut self) -> Option<usize> {
        Some(u16::from_be_bytes(self.array()?).into())
    }

    fn u32(&mut self) -> Option<usize> {
        usize::try_from(u32::from_be_bytes(self.array()?)).ok()
    }

    fn utf8(&mut self, n: usize) -> Option<String> {
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }

    fn latin1(&mut self, n: usize) -> Option<String> {
        Some(self.take(n)?.iter().map(|&b| char::from(b)).collect())
    }

    fn terms(&mut self, n: usize, depth: usize) -> Option<Vec<Term>> {
        (0..n).map(|_| self.term(depth)).collect()
    }

    fn term(&mut self, depth: usize) -> Option<Term> {
        if depth > MAX_DEPTH {
            return None;
        }
        let inner = depth + 1;
        Some(match self.u8()? {
            // SMALL_INTEGER_EXT and INTEGER_EXT
            97 => Term::Int(self.u8()? as i64),
            98 => Term::Int(i32::from_be_bytes(self.array()?).into()),
            // SMALL_BIG_EXT, which carries millisecond timestamps.
            110 => {
                let n = self.u8()?;
                let negative = self.u8()? != 0;
                let digits = self.take(n)?;
                if n > 8 {
                    return None;
                }
                let magnitude = digits
                    .iter()
                    .rev()
                    .fold(0u64, |acc, &digit| acc << 8 | u64::from(digit));
                let value = i64::try_from(magnitude).ok()?;
                Term::Int(if negative { -value } else { value })
            }
            // NEW_FLOAT_EXT
            70 => Term::Float(f64::from_be_bytes(self.array()?)),
            // ATOM_UTF8_EXT and SMALL_ATOM_UTF8_EXT, then the Latin-1 forms
            // older releases wrote.
            118 => {
                let n = self.u16()?;
                Term::Atom(self.utf8(n)?)
            }
            119 => {
                let n = self.u8()?;
                Term::Atom(self.utf8(n)?)
            }
            100 => {
                let n = self.u16()?;
                Term::Atom(self.latin1(n)?)
            }
            115 => {
                let n = self.u8()?;
                Term::Atom(self.latin1(n)?)
            }
            // BINARY_EXT
            109 => {
                let n = self.u32()?;
                Term::Binary(self.take(n)?.to_vec())
            }
            // NIL_EXT, STRING_EXT (a list of bytes) and LIST_EXT
            106 => Term::List(Vec::new()),
            107 => {
                let n = self.u16()?;
                Term::List(self.take(n)?.iter().map(|&b| Term::Int(b.into())).collect())
            }
            108 => {
                let n = self.u32()?;
                let items = self.terms(n, inner)?;
                // Only improper lists end in something other than [].
                if self.term(inner)? != Term::List(Vec::new()) {
                    return None;
                }
                Term::List(items)
            }
            // SMALL_TUPLE_EXT, LARGE_TUPLE_EXT and MAP_EXT
            104 => {
                let n = self.u8()?;
                Term::Tuple(self.terms(n, inner)?)
            }
            105 => {
                let n = self.u32()?;
                Term::Tuple(self.terms(n, inner)?)
            }
            116 => {
                let n = self.u32()?;
                let pairs = (0..n)
                    .map(|_| Some((self.term(inner)?, self.term(inner)?)))
                    .collect::<Option<_>>()?;
                Term::Map(pairs)
            }
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // :erlang.term_to_binary({%{api_key: "k", proxy_url: nil}, 1760600000000, :infinity})
    const SIGNED_RECORD: &[u8] = &[
        131, 104, 3, 116, 0, 0, 0, 2, 119, 7, 97, 112, 105, 95, 107, 101, 121, 109, 0, 0, 0, 1,
        107, 119, 9, 112, 114, 111, 120, 121, 95, 117, 114, 108, 119, 3, 110, 105, 108, 110, 6, 0,
        0, 6, 240, 235, 153, 1, 119, 8, 105, 110, 102, 105, 110, 105, 116, 121,
    ];
    // :erlang.term_to_binary([1, -2, 3.5, ~c"ab", "é", :ok, {}, 300])
    const MIXED_LIST: &[u8] = &[
        131, 108, 0, 0, 0, 8, 97, 1, 98, 255, 255, 255, 254, 70, 64, 12, 0, 0, 0, 0, 0, 0, 107,
        0, 2, 97, 98, 109, 0, 0, 0, 2, 195, 169, 119, 2, 111, 107, 104, 0, 98, 0, 0, 1, 44, 106,
    ];

    fn atom(name: &str) -> Term {
        Term::Atom(name.into())
    }

    #[test]
    fn reads_what_term_to_binary_writes() {
        assert_eq!(
            decode(SIGNED_RECORD),
            Some(Term::Tuple(vec![
                Term::Map(vec![
                    (atom("api_key"), Term::Binary(b"k".to_vec())),
                    (atom("proxy_url"), atom("nil")),
                ]),
                Term::Int(1_760_600_000_000),
                atom("infinity"),
            ]))
        );
        assert_eq!(
            decode(MIXED_LIST),
            Some(Term::List(vec![
                Term::Int(1),
                Term::Int(-2),
                Term::Float(3.5),
                Term::List(vec![Term::Int(97), Term::Int(98)]),
                Term::Binary("é".as_bytes().to_vec()),
                atom("ok"),
                Term::Tuple(Vec::new()),
                Term::Int(300),
            ]))
        );
    }

    #[test]
    fn looks_up_atom_keys() {
        let Some(Term::Tuple(items)) = decode(SIGNED_RECORD) else { panic!("not a tuple") };
        let record = &items[0];
        assert_eq!(record.get("api_key").and_then(Term::as_str), Some("k"));
        assert_eq!(record.get("proxy_url").and_then(Term::as_str), None, "nil is not a string");
        assert_eq!(record.get("pin"), None);
    }

    #[test]
    fn rejects_damaged_input() {
        assert_eq!(decode(&[]), None);
        assert_eq!(decode(&SIGNED_RECORD[1..]), None, "no version byte");
        assert_eq!(decode(&SIGNED_RECORD[..SIGNED_RECORD.len() - 1]), None, "truncated");
        assert_eq!(decode(&[SIGNED_RECORD, &[106]].concat()), None, "trailing bytes");
        // A list that claims four billion items but holds none.
        assert_eq!(decode(&[131, 108, 255, 255, 255, 255, 106]), None);
        // One-element tuples nested past MAX_DEPTH.
        let deep: Vec<u8> = [131]
            .into_iter()
            .chain([104, 1].repeat(MAX_DEPTH + 2))
            .chain([106])
            .collect();
        assert_eq!(decode(&deep), None);
    }
}
