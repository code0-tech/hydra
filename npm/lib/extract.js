'use strict';

const zlib = require('zlib');
const { Readable } = require('stream');
const tar = require('tar-stream');
const yauzl = require('yauzl');

// Pulls a single named file out of the gzip-compressed tarballs the release
// workflow produces.
function extractTarGz(buffer, fileName) {
  return new Promise((resolve, reject) => {
    const extract = tar.extract();
    let found = null;

    extract.on('entry', (header, stream, next) => {
      if (header.name !== fileName) {
        stream.resume();
        return next();
      }
      const chunks = [];
      stream.on('data', (c) => chunks.push(c));
      stream.on('end', () => {
        found = Buffer.concat(chunks);
        next();
      });
    });

    extract.on('finish', () => {
      if (!found) reject(new Error(`"${fileName}" not found in tar archive`));
      else resolve(found);
    });
    extract.on('error', reject);

    Readable.from(zlib.gunzipSync(buffer)).pipe(extract);
  });
}

// Pulls a single named file out of the zip archives the release workflow
// produces.
function extractZip(buffer, fileName) {
  return new Promise((resolve, reject) => {
    yauzl.fromBuffer(buffer, { lazyEntries: true }, (err, zipfile) => {
      if (err) return reject(err);

      zipfile.on('error', reject);
      zipfile.on('end', () => reject(new Error(`"${fileName}" not found in zip archive`)));

      zipfile.on('entry', (entry) => {
        if (entry.fileName !== fileName) {
          zipfile.readEntry();
          return;
        }
        zipfile.openReadStream(entry, (err, stream) => {
          if (err) return reject(err);
          const chunks = [];
          stream.on('data', (c) => chunks.push(c));
          stream.on('end', () => resolve(Buffer.concat(chunks)));
          stream.on('error', reject);
        });
      });

      zipfile.readEntry();
    });
  });
}

module.exports = { extractTarGz, extractZip };
