use std::fmt;
use std::io::{Read, Write};

use utils::Error;

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};

use super::errors::*;

#[cfg(test)]
mod header_test;

// PacketType specifies the type of an RTCP packet
// RTCP packet types registered with IANA. See: https://www.iana.org/assignments/rtp-parameters/rtp-parameters.xhtml#rtp-parameters-4

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PacketType {
    TypeUnsupported = 0,
    TypeSenderReport = 200,              // RFC 3550, 6.4.1
    TypeReceiverReport = 201,            // RFC 3550, 6.4.2
    TypeSourceDescription = 202,         // RFC 3550, 6.5
    TypeGoodbye = 203,                   // RFC 3550, 6.6
    TypeApplicationDefined = 204,        // RFC 3550, 6.7 (unimplemented)
    TypeTransportSpecificFeedback = 205, // RFC 4585, 6051
    TypePayloadSpecificFeedback = 206,   // RFC 4585, 6.3
}

// Transport and Payload specific feedback messages overload the count field to act as a message type. those are listed here
const FORMAT_SLI: u8 = 2;
const FORMAT_PLI: u8 = 1;
const FORMAT_TLN: u8 = 1;
const FORMAT_RRR: u8 = 5;
const FORMAT_REMB: u8 = 15;

impl fmt::Display for PacketType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let s = match self {
            PacketType::TypeUnsupported => "Unsupported",
            PacketType::TypeSenderReport => "SR",
            PacketType::TypeReceiverReport => "RR",
            PacketType::TypeSourceDescription => "SDES",
            PacketType::TypeGoodbye => "BYE",
            PacketType::TypeApplicationDefined => "APP",
            PacketType::TypeTransportSpecificFeedback => "TSFB",
            PacketType::TypePayloadSpecificFeedback => "PSFB",
        };
        write!(f, "{}", s)
    }
}

impl From<u8> for PacketType {
    fn from(b: u8) -> Self {
        match b {
            200 => PacketType::TypeSenderReport,       // RFC 3550, 6.4.1
            201 => PacketType::TypeReceiverReport,     // RFC 3550, 6.4.2
            202 => PacketType::TypeSourceDescription,  // RFC 3550, 6.5
            203 => PacketType::TypeGoodbye,            // RFC 3550, 6.6
            204 => PacketType::TypeApplicationDefined, // RFC 3550, 6.7 (unimplemented)
            205 => PacketType::TypeTransportSpecificFeedback, // RFC 4585, 6051
            206 => PacketType::TypePayloadSpecificFeedback, // RFC 4585, 6.3
            _ => PacketType::TypeUnsupported,
        }
    }
}

const RTP_VERSION: u8 = 2;

// A Header is the common header shared by all RTCP packets
#[derive(Debug, PartialEq)]
pub struct Header {
    // If the padding bit is set, this individual RTCP packet contains
    // some additional padding octets at the end which are not part of
    // the control information but are included in the length field.
    padding: bool,
    // The number of reception reports, sources contained or FMT in this packet (depending on the Type)
    count: u8,
    // The RTCP packet type for this packet
    packet_type: PacketType,
    // The length of this RTCP packet in 32-bit words minus one,
    // including the header and any padding.
    length: u16,
}

pub const HEADER_LENGTH: usize = 4;
const VERSION_SHIFT: u8 = 6;
const VERSION_MASK: u8 = 0x3;
const PADDING_SHIFT: u8 = 5;
const PADDING_MASK: u8 = 0x1;
const COUNT_SHIFT: u8 = 0;
const COUNT_MASK: u8 = 0x1f;
const COUNT_MAX: u8 = (1 << 5) - 1;

// Marshal encodes the Header in binary
impl Header {
    pub fn marshal<W: Write>(&self, writer: &mut W) -> Result<(), Error> {
        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |V=2|P|    RC   |   PT=SR=200   |             length            |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */

        let mut b0 = RTP_VERSION << VERSION_SHIFT;

        if self.padding {
            b0 |= 1 << PADDING_SHIFT
        }

        if self.count > 31 {
            return Err(ErrInvalidHeader.clone());
        }
        b0 |= self.count << COUNT_SHIFT;
        writer.write_u8(b0)?;

        let b1 = self.packet_type as u8;
        writer.write_u8(b1)?;

        writer.write_u16::<BigEndian>(self.length)?;

        Ok(())
    }

    // Unmarshal decodes the Header from binary
    pub fn unmarshal<R: Read>(reader: &mut R) -> Result<Self, Error> {
        /*
         *  0                   1                   2                   3
         *  0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         * |V=2|P|    RC   |      PT       |             length            |
         * +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
         */
        let b0 = reader.read_u8()?;
        let version = b0 >> VERSION_SHIFT & VERSION_MASK;
        if version != RTP_VERSION {
            return Err(ErrBadVersion.clone());
        }

        let padding = (b0 >> PADDING_SHIFT & PADDING_MASK) > 0;
        let count = b0 >> COUNT_SHIFT & COUNT_MASK;

        let b1 = reader.read_u8()?;
        let packet_type: PacketType = b1.into();
        if packet_type == PacketType::TypeUnsupported {
            return Err(ErrWrongType.clone());
        }

        let length = reader.read_u16::<BigEndian>()?;

        Ok(Header {
            padding,
            // The number of reception reports, sources contained or FMT in this packet (depending on the Type)
            count,
            // The RTCP packet type for this packet
            packet_type,
            // The length of this RTCP packet in 32-bit words minus one,
            // including the header and any padding.
            length,
        })
    }
}