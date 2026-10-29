'use strict';

const zlib = require('zlib');

// Minimal USTAR reader: enough to pull a single named file out of the
// gzip-compressed tarballs the release workflow produces. Avoids a
// dependency on the `tar` package for a one-entry archive.
function extractTarGz(buffer, fileName) {
  const tar = zlib.gunzipSync(buffer);
  let offset = 0;
  while (offset + 512 <= tar.length) {
    const header = tar.subarray(offset, offset + 512);
    if (header.every((b) => b === 0)) break; // end-of-archive marker

    const name = header.subarray(0, 100).toString('utf8').replace(/\0.*$/, '');
    const sizeOctal = header.subarray(124, 136).toString('utf8').replace(/\0.*$/, '').trim();
    const size = parseInt(sizeOctal, 8) || 0;
    const typeflag = String.fromCharCode(header[156]);

    const dataStart = offset + 512;
    if (name === fileName && (typeflag === '0' || typeflag === '\0')) {
      return tar.subarray(dataStart, dataStart + size);
    }

    offset = dataStart + Math.ceil(size / 512) * 512;
  }
  throw new Error(`"${fileName}" not found in tar archive`);
}

// Minimal ZIP reader for a single stored/deflated entry, using the
// central directory to find it. Avoids a dependency on a zip library.
function extractZip(buffer, fileName) {
  const EOCD_SIG = 0x06054b50;
  let eocdOffset = -1;
  for (let i = buffer.length - 22; i >= 0; i--) {
    if (buffer.readUInt32LE(i) === EOCD_SIG) {
      eocdOffset = i;
      break;
    }
  }
  if (eocdOffset === -1) throw new Error('not a valid zip file (no end-of-central-directory)');

  const entryCount = buffer.readUInt16LE(eocdOffset + 10);
  let cdOffset = buffer.readUInt32LE(eocdOffset + 16);

  for (let i = 0; i < entryCount; i++) {
    const sig = buffer.readUInt32LE(cdOffset);
    if (sig !== 0x02014b50) throw new Error('malformed zip central directory');

    const compressionMethod = buffer.readUInt16LE(cdOffset + 10);
    const compressedSize = buffer.readUInt32LE(cdOffset + 20);
    const nameLen = buffer.readUInt16LE(cdOffset + 28);
    const extraLen = buffer.readUInt16LE(cdOffset + 30);
    const commentLen = buffer.readUInt16LE(cdOffset + 32);
    const localHeaderOffset = buffer.readUInt32LE(cdOffset + 42);
    const name = buffer
      .subarray(cdOffset + 46, cdOffset + 46 + nameLen)
      .toString('utf8');

    if (name === fileName) {
      const lfNameLen = buffer.readUInt16LE(localHeaderOffset + 26);
      const lfExtraLen = buffer.readUInt16LE(localHeaderOffset + 28);
      const dataStart = localHeaderOffset + 30 + lfNameLen + lfExtraLen;
      const data = buffer.subarray(dataStart, dataStart + compressedSize);

      if (compressionMethod === 0) return data;
      if (compressionMethod === 8) return zlib.inflateRawSync(data);
      throw new Error(`unsupported zip compression method ${compressionMethod}`);
    }

    cdOffset += 46 + nameLen + extraLen + commentLen;
  }
  throw new Error(`"${fileName}" not found in zip archive`);
}

module.exports = { extractTarGz, extractZip };
