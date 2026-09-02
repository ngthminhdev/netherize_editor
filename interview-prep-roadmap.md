# Lộ trình ôn Thuật toán + System Design để đi phỏng vấn

> Cho: backend engineer ~2018–2026, mảng game/wallet/thanh toán realtime (Node.js, Redis, gRPC, pm2, GAMEMS adapter).
> Mục tiêu: **đổi việc**. Quỹ thời gian: **~10h/tuần** (1h/ngày T2–T6 + ~5h cuối tuần).
> Tổng: **14 tuần**. Ngày bắt đầu: điền vào đây → `____/____/2026`.

---

## 0. Đọc trước khi bắt đầu

### Điểm mạnh sẵn có (phải khai thác, đừng học lại)
Bạn đã có thứ 90% ứng viên không có: **kinh nghiệm production thật với tiền thật**.
- Đã truy vết mất tiền do fire-and-forget call (PI-149: orphan reserve 2,540,000 trong Redis).
- Đã tìm root cause SEV-1: adapter crash-loop vì `.env` hỏng lúc deploy → 684 ticket kẹt `Running`, 153.7M VND (ktrng_3932).
- Hiểu thực chiến: idempotency, retry, reserve/commit/release, TTL cache vs upstream timeout, race condition, log correlation UTC vs local.

→ Đây chính là **nguyên liệu cho vòng System Design và vòng Behavioral**. Không cần bịa project.

### Điểm yếu cần lấp (giả định, tuần 1 sẽ kiểm chứng)
1. **DSA dạng thi**: lâu không code giải thuật dưới áp lực 25 phút.
2. **Trình bày System Design có cấu trúc**: biết làm, nhưng chưa quen nói thành 45 phút mạch lạc theo framework.
3. **Chiều rộng**: hệ bạn làm là 1 domain sâu; phỏng vấn hỏi cả feed, search, storage, streaming.

### Nguyên tắc xuyên suốt
- **Nói ra miệng.** Giải thuật: đọc to hướng tiếp cận trước khi gõ. System design: nói + vẽ. Câm lặng code = trượt.
- **Timebox thật.** Bấm giờ. Quá giờ thì dừng, xem lời giải, ghi lại vì sao bí.
- **Không cày số lượng.** 150 bài hiểu sâu > 500 bài làm vẹt.
- **Sổ tay lỗi** (`~/Work/docs/interview-notes.md`): mỗi bài sai ghi 3 dòng — bí ở đâu, pattern đúng là gì, dấu hiệu nhận biết lần sau.

---

## 1. Cấu trúc tuần cố định

| Ngày | Thời lượng | Nội dung |
|---|---|---|
| T2 | 1h | DSA — 2 bài theo pattern của tuần |
| T3 | 1h | DSA — 2 bài (1 bài medium mới, 1 bài làm lại từ sổ lỗi) |
| T4 | 1h | System Design — đọc/học 1 khái niệm + vẽ |
| T5 | 1h | DSA — 2 bài |
| T6 | 1h | System Design — 1 case, viết dàn ý 45 phút |
| T7 | 3h | Mock: 1 buổi DSA 45p bấm giờ + 1 buổi SD 45p nói to (ghi âm) |
| CN | 2h | Ôn sổ lỗi + spaced repetition + viết lại 1 story behavioral |

Nghỉ hẳn 1 ngày/tuần nếu đuối. Tuần bỏ lỡ thì dời, không nhồi.

---

## 2. Giai đoạn 1 — Nền tảng DSA (Tuần 1–6)

Mục tiêu: **~90 bài**, phủ hết pattern hay hỏng. Nguồn chính: NeetCode 150 (thứ tự có sẵn), phụ: LeetCode Top Interview 150.

Ngôn ngữ thi: **Node/JavaScript** (chốt rồi, không đổi giữa chừng). **Go** là ngôn ngữ phụ — học đọc/viết được, dùng cho phần concurrency và để mở thêm cửa job, **không** dùng để thi DSA.

#### Node — đồ nghề thiếu, phải tự chuẩn bị trước tuần 1

JS thiếu vài thứ Python có sẵn. Chuẩn bị 1 file `snippets.js` thuộc lòng, gõ lại được trong 2 phút:

```js
// 1. Min-heap — JS KHÔNG có sẵn. Thuộc lòng cái này, dùng cho Top-K / Dijkstra.
class Heap {                       // cmp(a,b) < 0 => a lên trước
  constructor(cmp = (a, b) => a - b) { this.a = []; this.cmp = cmp; }
  get size() { return this.a.length; }
  peek() { return this.a[0]; }
  push(v) {
    this.a.push(v);
    for (let i = this.a.length - 1; i > 0;) {
      const p = (i - 1) >> 1;
      if (this.cmp(this.a[i], this.a[p]) >= 0) break;
      [this.a[i], this.a[p]] = [this.a[p], this.a[i]]; i = p;
    }
  }
  pop() {
    const top = this.a[0], last = this.a.pop();
    if (this.a.length) {
      this.a[0] = last;
      for (let i = 0;;) {
        const l = 2 * i + 1, r = l + 1; let m = i;
        if (l < this.a.length && this.cmp(this.a[l], this.a[m]) < 0) m = l;
        if (r < this.a.length && this.cmp(this.a[r], this.a[m]) < 0) m = r;
        if (m === i) break;
        [this.a[i], this.a[m]] = [this.a[m], this.a[i]]; i = m;
      }
    }
    return top;
  }
}

// 2. Deque cho BFS — KHÔNG dùng shift() (O(n) => BFS thành O(n²)).
const q = [start]; let head = 0;
while (head < q.length) { const cur = q[head++]; /* ... */ }

// 3. Sort số — mặc định JS sort theo CHUỖI: [10,9] => [10,9]. Luôn truyền comparator.
arr.sort((a, b) => a - b);
pairs.sort((a, b) => a[0] - b[0] || a[1] - b[1]);

// 4. Mảng 2 chiều — Array(n).fill(Array(m)) là BUG (dùng chung 1 ref).
const dp = Array.from({ length: n }, () => Array(m).fill(0));

// 5. Map cho key số/tuple (giữ thứ tự chèn); object ép key thành string.
const cnt = new Map();
cnt.set(k, (cnt.get(k) ?? 0) + 1);
```

Bẫy còn lại, nhớ đầu:
- Số nguyên chỉ chính xác tới `2^53` (`Number.MAX_SAFE_INTEGER`). Đề có tích lớn → `BigInt`.
- Đệ quy sâu ~10k frame là tràn stack. DFS trên 10^5 node → viết vòng lặp + stack thủ công.
- `arr.includes` là O(n); cần O(1) thì `Set`.
- Nối chuỗi trong vòng lặp → dồn vào mảng rồi `.join('')`.
- Vô cực: `Infinity` / `-Infinity`, không cần hằng số giả.
- Chia lấy nguyên: `Math.floor(a / b)`, hoặc `(a / b) | 0` khi chắc chắn trong 32-bit.

Buổi T2 tuần 1: gõ lại `Heap` từ đầu, không nhìn. Lặp lại đầu tuần 5 (tuần Heap).

#### Go — học phụ, 30 phút/tuần từ tuần 7

Không thi bằng Go. Mục đích: đọc được code Go, và nói được về concurrency model khi phỏng vấn (nhiều backend game/fintech đang là Go — kể cả phía `userAgent "go"` bạn thấy trong log GAMEMS).

Đủ dùng, không hơn:
- Tour of Go (2–3 buổi) → syntax, slice/map, struct, interface, `error` trả về tường minh.
- goroutine + channel + `select`; `sync.WaitGroup`, `sync.Mutex`.
- `context.Context` cho timeout/cancel — đây là thứ hay được hỏi và rất hợp domain của bạn (huỷ lệnh chuyển tiền khi hết hạn).
- Viết lại **1** thứ đã có bằng Node sang Go: một worker retry đọc queue, có backoff + `context` timeout. Chỉ 1 cái, đủ để kể trong phỏng vấn.

So sánh phải nói trôi khi bị hỏi: Node = event loop đơn luồng + async I/O, CPU-bound thì nghẽn; Go = goroutine trên nhiều core, song song thật, nhưng phải tự lo data race.

| Tuần | Pattern | Số bài | Chốt kiến thức |
|---|---|---|---|
| 1 | Array/Hash Map, Two Pointers | 14 | Đổi O(n²)→O(n) bằng hash; template two pointer |
| 2 | Sliding Window, Stack | 14 | Window co giãn; monotonic stack |
| 3 | Binary Search, Linked List | 14 | Bounds `lo/hi` không lỗi off-by-one; fast/slow pointer |
| 4 | Tree (DFS/BFS), Trie | 16 | Đệ quy trên cây; BFS theo tầng |
| 5 | Heap, Backtracking, Interval | 16 | Top-K bằng heap; cắt nhánh; merge interval |
| 6 | Graph (BFS/DFS/Topo/Union-Find) | 16 | Topological sort; detect cycle; DSU |

**Quy tắc mỗi bài (25 phút cứng):**
1. 3 phút: đọc đề, nói to hướng làm + độ phức tạp *trước khi* gõ.
2. 15 phút: code.
3. 5 phút: tự test bằng edge case (rỗng, 1 phần tử, trùng, âm, tràn).
4. 2 phút: xem lời giải tối ưu, ghi sổ lỗi nếu lệch.

Quá 25 phút → dừng, đọc lời giải, đánh dấu `#redo`, làm lại sau 3 ngày và sau 2 tuần.

**Checkpoint cuối tuần 6:** tự làm 1 bộ 2 bài medium trong 45 phút, không gợi ý. Đạt ≥1.5/2 mới sang giai đoạn 2. Không đạt → thêm 1 tuần vá pattern yếu nhất, không kéo cả lộ trình.

---

## 3. Giai đoạn 2 — System Design (Tuần 7–11)

Đây là vòng bạn có thể **ăn điểm mạnh nhất**. Đừng học kiểu đọc blog, học kiểu **vẽ + nói + bảo vệ trade-off**.

### Framework 45 phút (thuộc lòng, dùng cho mọi câu)

```
1. Làm rõ yêu cầu           (5')  — chức năng, phi chức năng, ai dùng, cái gì NGOÀI phạm vi
2. Ước lượng quy mô         (5')  — DAU, QPS đọc/ghi, dung lượng/ngày, peak = 3-5x trung bình
3. API + mô hình dữ liệu    (5')  — endpoint/gRPC method, schema, khoá chính, index
4. Kiến trúc mức cao        (10') — vẽ box: client → LB → service → cache → DB → queue
5. Đào sâu 1-2 điểm         (15') — phỏng vấn viên chọn, hoặc bạn đề xuất chỗ khó nhất
6. Nút cổ chai + đánh đổi   (5')  — cái gì hỏng trước? scale ra sao? nhất quán hay sẵn sàng?
```

Câu hỏi phải tự hỏi ở mọi bài: *cái gì xảy ra khi request này chết giữa chừng?* — đây là câu bạn trả lời tốt hơn người khác, vì bạn đã sống với nó.

### Nền tảng lý thuyết (T4 hàng tuần)

| Tuần | Chủ đề | Đích đến |
|---|---|---|
| 7 | Scaling cơ bản: LB, replication, sharding, CAP, consistency levels | Nói được khác nhau strong / eventual / read-your-writes |
| 8 | Caching + queue: cache-aside vs write-through, invalidation, Kafka/RabbitMQ, at-least-once vs exactly-once | Giải thích được vì sao `TRANSACTION_CACHE_TTL 5s < upstream timeout 30s` là bug |
| 9 | Storage: SQL vs NoSQL, index, LSM vs B-tree, hot partition | Chọn được DB và bảo vệ lựa chọn |
| 10 | Reliability: idempotency, outbox pattern, saga, circuit breaker, backpressure, graceful degradation | Thiết kế được settle-retry queue |
| 11 | Vận hành: observability, rate limit, deploy an toàn, blue-green, feature flag | Kể được cách chặn sự cố `.env` hỏng |

Nguồn: *Designing Data-Intensive Applications* (chương 5–9 là phần đáng tiền nhất — đọc chọn lọc, đừng đọc tuần tự cả cuốn), ByteByteGo/Hello Interview cho format bài mẫu.

### Case tập (T6 hàng tuần — mỗi tuần 1 case, viết dàn ý rồi nói to 45')

1. **Ví điện tử / hệ thống chuyển tiền** ← case chủ lực của bạn. Phải hoàn hảo.
2. Rút gọn URL (bài khởi động, luyện framework)
3. Rate limiter phân tán
4. Bảng xếp hạng realtime (đúng domain game)
5. Feed mạng xã hội (luyện fanout, ngoài vùng an toàn)
6. Hệ thống chat/notification (WebSocket, delivery guarantee)
7. Thu thập & xử lý log/metrics quy mô lớn

Case #1 làm 2 lần: tuần 7 và tuần 11, so 2 bản để thấy tiến bộ.

### Bộ vũ khí riêng — chuẩn bị kỹ, gần như chắc chắn được hỏi

Với nền tảng của bạn, hãy chủ động lái về **hệ thống tiền không được sai**:

- **Reserve → Commit → Release** là saga 2 pha. Vẽ được state machine đầy đủ, kể cả nhánh lỗi.
- **Idempotency key**: mọi lệnh chuyển tiền mang khoá duy nhất; retry phải trả cùng kết quả. `409 EXISTED` **là thành công**, không phải lỗi — chính là bug bạn từng thấy sinh ra orphan `Reserve_<uid>`.
- **Không bao giờ fire-and-forget lệnh tiền.** Cần outbox/queue bền + worker retry với backoff. Đây là PI-149.
- **Ledger append-only + reconciliation job**: nguồn sự thật là sổ cái, số dư là kết quả tính. Job đối soát định kỳ phát hiện lệch.
- **Downstream chết thì sao?** Retry queue có DLQ, circuit breaker, và **settle không được rơi vào khoảng trống deploy**. Đây là ktrng_3932.
- **Exactly-once là ảo tưởng** — chỉ có at-least-once + idempotent consumer.

---

## 4. Giai đoạn 3 — Đánh bóng & thi thật (Tuần 12–14)

| Tuần | Việc |
|---|---|
| 12 | 3 mock full (DSA 45' + SD 45' + behavioral 30'). Dùng Pramp/interviewing.io hoặc bạn nghề. Sửa theo feedback. |
| 13 | Nộp CV **loạt công ty warm-up trước**, công ty mơ ước xếp sau ~2 tuần. Ôn sổ lỗi, redo toàn bộ bài `#redo`. |
| 14 | Phỏng vấn thật. Mỗi buổi xong ghi lại câu bị hỏi trong 30 phút, còn nóng. |

### Behavioral — viết sẵn 5 story theo STAR (làm rải rác từ tuần 7, mỗi CN 1 story)

Bạn có sẵn nguyên liệu, chỉ cần đóng gói:
1. **Sự cố nghiêm trọng nhất bạn xử lý** → ktrng_3932: 684 ticket kẹt, 153.7M VND, truy log pm2 đối chiếu UTC/VN, tìm ra `.env` hỏng lúc deploy.
2. **Lần bạn phát hiện ticket mô tả sai vấn đề** → PI-149: ticket ghi lệch 540k, bạn chứng minh thực tế là 2,540,000 do so sánh sai mốc thời gian. Điểm nhấn: *dữ liệu thắng giả định*.
3. **Bug khó nhất từng debug** → race condition RESERVE chậm vs RELEASE nhanh, lua script xoá field không tồn tại vẫn báo thành công.
4. **Lần bạn cải thiện quy trình** → validate `.env` theo `env.dist` trước khi pm2 restart; hotfix chưa lên `master-v2` → nhận diện lỗ hổng quy trình release.
5. **Bất đồng với đồng nghiệp / lựa chọn kỹ thuật khó** → tự chọn.

Mỗi story: **2 phút nói**, có con số cụ thể, kết thúc bằng *bài học rút ra*.

### CV — 3 điều phải sửa
- Mỗi gạch đầu dòng có **số**: "truy vết 684 giao dịch kẹt trị giá 153.7M VND", không phải "xử lý sự cố hệ thống".
- Nêu rõ **quy mô**: QPS, số user đồng thời, throughput ví. Chu kỳ Sicbo ~66s với burst 450–630 transfer mỗi vòng settle là con số ấn tượng — dùng nó.
- Từ khoá cho ATS: distributed systems, gRPC, Redis, idempotency, transaction consistency, incident response, high-throughput payments.

---

## 5. Theo dõi tiến độ

Đánh dấu vào đây mỗi cuối tuần:

```
[ ] T1  DSA: Array/Hash, Two Pointers      (__/14)   Node — gõ lại Heap không nhìn: [ ]
[ ] T2  DSA: Sliding Window, Stack         (__/14)
[ ] T3  DSA: Binary Search, Linked List    (__/14)
[ ] T4  DSA: Tree, Trie                    (__/16)
[ ] T5  DSA: Heap, Backtracking, Interval  (__/16)
[ ] T6  DSA: Graph                         (__/16)   ✅ CHECKPOINT: 2 medium / 45'
[ ] T7  SD: Scaling      + case Ví điện tử (bản 1)      Go: Tour of Go
[ ] T8  SD: Cache/Queue  + case Rút gọn URL             Go: goroutine + channel
[ ] T9  SD: Storage      + case Rate limiter            Go: context, timeout
[ ] T10 SD: Reliability  + case Leaderboard             Go: viết retry worker
[ ] T11 SD: Vận hành     + case Ví điện tử (bản 2) — so với bản 1
[ ] T12 3 mock full + sửa theo feedback
[ ] T13 Nộp CV + redo toàn bộ #redo
[ ] T14 Phỏng vấn thật
```

---

## 6. Bẫy cần tránh

- **Cày LeetCode tới ngày cuối** rồi bỏ bê System Design. Với ~8 năm kinh nghiệm, **vòng System Design + Behavioral quyết định level và lương**, DSA chỉ là cửa lọc.
- **Học lý thuyết mà không vẽ.** Không vẽ được thì chưa hiểu.
- **Kể sự cố mà không có con số.** "Rất nhiều giao dịch bị kẹt" yếu; "684 ticket, 647 user, 153.7M VND" mạnh.
- **Thi DSA bằng Go.** Đã chốt Node. Go chỉ để đọc, để kể chuyện concurrency, và để mở thêm cửa job — không mang vào phòng thi giải thuật.
- **Sa đà vào Go.** 30 phút/tuần là trần. Vượt trần là đang trốn việc khó (System Design).
- **Chờ "sẵn sàng" mới nộp CV.** Nộp từ tuần 13, mấy buổi đầu chính là mock chất lượng cao nhất.
