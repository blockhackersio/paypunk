// bc-ur fountain decoder — vendored as ESM to avoid Vite CJS interop issues
import BigNumber from 'bignumber.js';
import aliasSample from '@keystonehq/alias-sampling';

function crc32(data: Buffer | Uint8Array): number {
  let crc = 0xffffffff;
  for (let i = 0; i < data.length; i++) {
    crc ^= data[i];
    for (let j = 0; j < 8; j++) {
      crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    }
  }
  return (crc ^ 0xffffffff) >>> 0;
}

function bufferXOR(a: Buffer, b: Buffer): Buffer {
  const len = Math.min(a.length, b.length);
  const result = Buffer.alloc(len);
  for (let i = 0; i < len; i++) result[i] = a[i] ^ b[i];
  return result;
}

function arrayContains(container: number[], containee: number[]): boolean {
  for (const c of containee) if (!container.includes(c)) return false;
  return true;
}

function setDifference(a: number[], b: number[]): number[] {
  return a.filter(x => !b.includes(x));
}

function intToBytesBE(num: number): Buffer {
  const buf = Buffer.alloc(4);
  buf.writeUInt32BE(num, 0);
  return buf;
}

async function sha256Hash(data: Buffer): Promise<Buffer> {
  const hash = await crypto.subtle.digest('SHA-256', new Uint8Array(data));
  return Buffer.from(hash);
}

function chooseDegree(seqLength: number, rng: { nextDouble: () => BigNumber }): number {
  const probs: number[] = [];
  for (let i = 0; i < seqLength; i++) probs.push(1 / (i + 1));
  const sampler = aliasSample(probs, undefined, () => rng.nextDouble().toNumber());
  return sampler.next() + 1;
}

function shuffle<T>(items: T[], rng: { nextInt: (lo: number, hi: number) => number }): T[] {
  const remaining = [...items];
  const result: T[] = [];
  while (remaining.length > 0) {
    const index = rng.nextInt(0, remaining.length - 1);
    result.push(remaining.splice(index, 1)[0]);
  }
  return result;
}

class Xoshiro {
  private s: bigint[];

  constructor(seed: Buffer) {
    this.s = [0n, 0n, 0n, 0n];
    for (let i = 0; i < 4; i++) {
      let v = 0n;
      for (let n = 0; n < 8; n++) {
        v = (v << 8n) | BigInt(seed[i * 8 + n]);
      }
      this.s[i] = v & 0xFFFFFFFFFFFFFFFFn;
    }
  }

  private rotl(x: bigint, k: bigint): bigint {
    return ((x << k) | (x >> (64n - k))) & 0xFFFFFFFFFFFFFFFFn;
  }

  roll(): bigint {
    const result = (this.rotl((this.s[1] * 5n) & 0xFFFFFFFFFFFFFFFFn, 7n) * 9n) & 0xFFFFFFFFFFFFFFFFn;
    const t = (this.s[1] << 17n) & 0xFFFFFFFFFFFFFFFFn;
    this.s[2] = (this.s[2] ^ this.s[0]) & 0xFFFFFFFFFFFFFFFFn;
    this.s[3] = (this.s[3] ^ this.s[1]) & 0xFFFFFFFFFFFFFFFFn;
    this.s[1] = (this.s[1] ^ this.s[2]) & 0xFFFFFFFFFFFFFFFFn;
    this.s[0] = (this.s[0] ^ this.s[3]) & 0xFFFFFFFFFFFFFFFFn;
    this.s[2] = (this.s[2] ^ t) & 0xFFFFFFFFFFFFFFFFn;
    this.s[3] = this.rotl(this.s[3], 45n);
    return result;
  }

  nextDouble(): BigNumber {
    return new BigNumber(this.roll().toString()).div(new BigNumber(2).pow(64));
  }

  nextInt(low: number, high: number): number {
    return Math.floor(this.nextDouble().toNumber() * (high - low + 1)) + low;
  }
}

async function chooseFragments(seqNum: number, seqLength: number, checksum: number): Promise<number[]> {
  if (seqNum <= seqLength) {
    return [seqNum - 1];
  }
  const seed = Buffer.concat([intToBytesBE(seqNum), intToBytesBE(checksum)]);
  const hash = await sha256Hash(seed);
  const rng = new Xoshiro(hash);
  const degree = chooseDegree(seqLength, rng);
  const indexes = [...Array(seqLength).keys()];
  const shuffled = shuffle(indexes, rng);
  return shuffled.slice(0, degree);
}

const BYTEWORDS = 'ableacidalsoapexaquaarchatomauntawayaxisbackbaldbarnbeltbetabiasbluebodybragbrewbulbbuzzcalmcashcatschefcityclawcodecolacookcostcruxcurlcuspcyandarkdatadaysdelidicedietdoordowndrawdropdrumdulldutyeacheasyechoedgeepicevenexamexiteyesfactfairfernfigsfilmfishfizzflapflewfluxfoxyfreefrogfuelfundgalagamegeargemsgiftgirlglowgoodgraygrimgurugushgyrohalfhanghardhawkheathelphighhillholyhopehornhutsicedideaidleinchinkyintoirisironitemjadejazzjoinjoltjowljudojugsjumpjunkjurykeepkenokeptkeyskickkilnkingkitekiwiknoblamblavalazyleaflegsliarlimplionlistlogoloudloveluaulucklungmainmanymathmazememomenumeowmildmintmissmonknailnavyneednewsnextnoonnotenumbobeyoboeomitonyxopenovalowlspaidpartpeckplaypluspoempoolposepuffpumapurrquadquizraceramprealredorichroadrockroofrubyruinrunsrustsafesagascarsetssilkskewslotsoapsolosongstubsurfswantacotasktaxitenttiedtimetinytoiltombtoystriptunatwinuglyundouniturgeuservastveryvetovialvibeviewvisavoidvowswallwandwarmwaspwavewaxywebswhatwhenwhizwolfworkyankyawnyellyogayurtzapszerozestzinczonezoom';
const WORD_LEN = 4;

function getWord(index: number): string {
  return BYTEWORDS.slice(index * WORD_LEN, (index + 1) * WORD_LEN);
}

function encodeMinimal(input: string): string {
  let out = '';
  for (let i = 0; i < input.length; i++) {
    const val = parseInt(input[i], 16);
    const word = getWord(val);
    out += word[0] + word[WORD_LEN - 1];
  }
  return out;
}

function decodeMinimal(input: string): Buffer {
  const bytes: number[] = [];
  for (let i = 0; i < input.length; i += 2) {
    const pair = input.slice(i, i + 2);
    for (let j = 0; j < 256; j++) {
      const word = BYTEWORDS.slice(j * WORD_LEN, (j + 1) * WORD_LEN);
      if (word[0] === pair[0] && word[WORD_LEN - 1] === pair[1]) {
        bytes.push(j);
        break;
      }
    }
  }
  return Buffer.from(bytes);
}

function cborDecode(buf: Buffer): any[] {
  const dv = new DataView(buf.buffer, buf.byteOffset, buf.byteLength);
  let offset = 0;
  function read(): any {
    const ib = buf[offset++];
    const major = ib >> 5;
    const info = ib & 0x1f;
    let value: number;
    if (info < 24) value = info;
    else if (info === 24) value = buf[offset++];
    else if (info === 25) { value = dv.getUint16(offset); offset += 2; }
    else if (info === 26) { value = dv.getUint32(offset); offset += 4; }
    else throw new Error('Unsupported CBOR');
    if (major === 0) return value;
    if (major === 1) return -1 - value;
    if (major === 2) {
      const bytes = buf.slice(offset, offset + value);
      offset += value;
      return bytes;
    }
    if (major === 4) {
      const arr: any[] = [];
      for (let i = 0; i < value; i++) arr.push(read());
      return arr;
    }
    throw new Error(`Unsupported CBOR major type ${major}`);
  }
  return read();
}

function cborEncodeBytes(data: Buffer): Buffer {
  const len = data.length;
  let header: Buffer;
  if (len < 24) header = Buffer.from([0x40 | len]);
  else if (len < 256) header = Buffer.from([0x58, len]);
  else header = Buffer.from([0x59, (len >> 8) & 0xff, len & 0xff]);
  return Buffer.concat([header, data]);
}

interface FountainPart {
  seqNum: number;
  seqLength: number;
  messageLength: number;
  checksum: number;
  fragment: Buffer;
}

function parsePart(ur: string): FountainPart | null {
  try {
    const lower = ur.toLowerCase();
    if (!lower.startsWith('ur:')) return null;
    const comps = lower.slice(3).split('/');
    if (comps.length < 2) return null;

    // Single-part: ur:bytes/<bytewords>
    if (comps.length === 2) {
      const bw = decodeMinimal(comps[1]);
      const hex = bw.toString('hex');
      const cbor = Buffer.from(hex, 'hex');
      const decoded = cborDecode(cbor);
      return {
        seqNum: 0,
        seqLength: 1,
        messageLength: (decoded as unknown as Buffer).length,
        checksum: 0,
        fragment: decoded as unknown as Buffer,
      };
    }

    // Multi-part: ur:bytes/<seqNum-seqLength>/<bytewords>
    const seqPart = comps[1].split('-');
    if (seqPart.length !== 2) return null;
    const fragmentBw = comps.slice(2).join('/');
    const cborBuf = decodeMinimal(fragmentBw);
    const decoded: any[] = cborDecode(cborBuf);
    if (!Array.isArray(decoded) || decoded.length < 5) return null;
    return {
      seqNum: decoded[0] as number,
      seqLength: decoded[1] as number,
      messageLength: decoded[2] as number,
      checksum: decoded[3] as number,
      fragment: decoded[4] as Buffer,
    };
  } catch { return null; }
}

export function createEncoder(bytes: Uint8Array) {
  const cbor = cborEncodeBytes(Buffer.from(bytes));
  const hex = cbor.toString('hex');
  const bw = encodeMinimal(hex);
  const part = `ur:bytes/${bw}`;
  return {
    nextPart: () => part,
    get fragmentCount() { return 1; },
  };
}

export function createDecoder() {
  const simpleParts = new Map<number, Buffer>();
  const mixedParts: { indexes: number[]; fragment: Buffer }[] = [];
  let expectedSeqLength = 0;
  let expectedMessageLength = 0;
  let expectedChecksum = 0;
  let expectedFragmentLen = 0;
  let result: Buffer | null = null;
  let processedCount = 0;
  let processing = false;
  const pending: FountainPart[] = [];

  async function processPart(part: FountainPart) {
    if (result) return;
    if (expectedSeqLength === 0) {
      expectedSeqLength = part.seqLength;
      expectedMessageLength = part.messageLength;
      expectedChecksum = part.checksum;
      expectedFragmentLen = part.fragment.length;
    } else if (
      part.seqLength !== expectedSeqLength ||
      part.messageLength !== expectedMessageLength ||
      part.checksum !== expectedChecksum ||
      part.fragment.length !== expectedFragmentLen
    ) {
      return;
    }

    const indexes = await chooseFragments(part.seqNum, part.seqLength, part.checksum);
    const fragment = part.fragment;

    let finalIndexes = indexes;
    let finalFragment = fragment;
    for (const [idx, f] of simpleParts) {
      if (finalIndexes.includes(idx)) {
        finalIndexes = finalIndexes.filter(i => i !== idx);
        finalFragment = bufferXOR(finalFragment, f);
      }
    }
    for (const mp of mixedParts) {
      if (arrayContains(finalIndexes, mp.indexes)) {
        finalIndexes = setDifference(finalIndexes, mp.indexes);
        finalFragment = bufferXOR(finalFragment, mp.fragment);
      }
    }

    if (finalIndexes.length === 1) {
      const idx = finalIndexes[0];
      if (!simpleParts.has(idx)) {
        simpleParts.set(idx, finalFragment);
        for (let i = mixedParts.length - 1; i >= 0; i--) {
          const mp = mixedParts[i];
          if (mp.indexes.includes(idx)) {
            const newIndexes = mp.indexes.filter(i => i !== idx);
            const newFragment = bufferXOR(mp.fragment, simpleParts.get(idx)!);
            mixedParts.splice(i, 1);
            if (newIndexes.length === 1) {
              if (!simpleParts.has(newIndexes[0])) {
                simpleParts.set(newIndexes[0], newFragment);
              }
            } else {
              mixedParts.push({ indexes: newIndexes, fragment: newFragment });
            }
          }
        }
        if (simpleParts.size === expectedSeqLength) {
          const sorted = [...simpleParts.entries()].sort((a, b) => a[0] - b[0]);
          const message = Buffer.concat(sorted.map(([, f]) => f)).slice(0, expectedMessageLength);
          const checksum = crc32(message);
          if (checksum === expectedChecksum) {
            result = message;
          }
        }
      }
    } else if (finalIndexes.length > 1) {
      mixedParts.push({ indexes: finalIndexes, fragment: finalFragment });
    }
  }

  async function processPending() {
    if (processing) return;
    processing = true;
    while (pending.length > 0 && !result) {
      const part = pending.shift()!;
      await processPart(part);
    }
    processing = false;
  }

  return {
    receive(part: string) {
      const parsed = parsePart(part);
      if (parsed) {
        processedCount++;
        pending.push(parsed);
        processPending();
      }
    },
    get progress() {
      if (result) return 1;
      if (expectedSeqLength === 0) return 0;
      return Math.min(0.99, processedCount / (expectedSeqLength * 1.75));
    },
    get isComplete() { return result !== null; },
    get result(): Uint8Array | null {
      if (!result) return null;
      return new Uint8Array(result);
    },
  };
}
