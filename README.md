# Batch Helper

**Batch Helper** là ứng dụng desktop dành cho Windows để quản lý, chạy và theo dõi các dự án Java/Apache Ant. Giao diện mô phỏng phong cách Windows 95/98 và hỗ trợ nhiều dự án qua hệ thống tab.

Dự án được xây dựng bằng **Tauri 2**, **Rust**, **Vite**, **JavaScript**, **HTML** và **CSS**.

## Tính năng

- Quản lý nhiều thư mục dự án bằng tab.
- Tự động quét `build.xml` và danh sách target Ant.
- Chạy Apache Ant build và hiển thị kết quả/log theo từng tab.
- Tìm, xem, lọc và chạy các file `.bat` thuộc thư mục build.
- Chọn nhiều file `.bat`, chạy tuần tự và xem kết quả từng bước.
- Tạo nhóm `.bat` để chạy nhiều script theo thứ tự đã lưu.
- Kiểm tra môi trường Java/JDK, Ant và cấu hình `JAVA_HOME` / `ANT_HOME`.
- Tự động cấu hình một số thiết lập trong dự án Java Ant.
- Kiểm tra thông tin database từ file `.properties` và thử kết nối.
- Tùy chỉnh kích thước cửa sổ Command Prompt và thư mục lưu ảnh screenshot.
- Hỗ trợ lưu lịch sử dự án, tab đang mở và cấu hình người dùng.

## Công nghệ

- **Frontend:** HTML, CSS, JavaScript, Vite
- **Desktop runtime:** Tauri 2
- **Backend:** Rust 2021
- **Build tool:** Apache Ant
- **Giao diện:** 98.css
- **Nền tảng chính:** Windows

## Yêu cầu hệ thống

- Windows 10/11
- Node.js 18 trở lên
- npm đi kèm Node.js
- Rust và Cargo
- Microsoft Visual Studio Build Tools hoặc Visual Studio với component **Desktop development with C++**
- WebView2 Runtime (thường đã có trên Windows 10/11)
- Java JDK 8 trở lên nếu dự án Java của bạn cần
- Apache Ant hoặc thư mục Ant được phát hiện/cung cấp trong ứng dụng

### Kiểm tra công cụ

```powershell
node --version
npm --version
rustc --version
cargo --version
java -version
javac -version
ant -version
```

## Cài đặt Rust

Nếu máy chưa có Rust:

```powershell
winget install Rustlang.Rustup
rustup default stable
rustup update
```

Khởi động lại terminal sau khi cài đặt.

## Cài đặt dự án

Clone repository:

```powershell
git clone https://github.com/QuangThe0412/bach_helper.git
cd bach_helper
```

Cài các package npm:

```powershell
npm install
```

Hoặc dùng lockfile hiện có:

```powershell
npm ci
```

## Chạy ở chế độ phát triển

```powershell
npm run tauri dev
```

Vite sẽ chạy frontend tại `http://localhost:5173`. Tauri cũng có thể mở cửa sổ ứng dụng khi backend Rust đã khởi động thành công.

Cổng `5173` đã được cấu hình là cổng bắt buộc trong `vite.config.js`.

## Chạy frontend riêng

Chỉ để kiểm tra giao diện web:

```powershell
npm run dev
```

Sau đó truy cập:

```text
http://localhost:5173
```

Lưu ý: các chức năng gọi Tauri như quét dự án, chạy Ant hoặc quản lý file sẽ không hoạt động đầy đủ khi chỉ chạy trình duyệt thông thường.

## Build ứng dụng

Build toàn bộ ứng dụng desktop:

```powershell
npm run tauri build
```

Hoặc build frontend trước:

```powershell
npm run build
```

Các file frontend sau build nằm trong thư mục `dist/`. Bản cài đặt Tauri được tạo trong thư mục:

```text
src-tauri/target/release/bundle/
```

Tùy target trong môi trường Windows, Tauri có thể tạo MSI hoặc NSIS installer trong thư mục bundle.

## Sử dụng cơ bản

1. Mở ứng dụng.
2. Chọn **Browse Folder** để chọn thư mục dự án Java chứa `build.xml`.
3. Chọn Ant target cần chạy.
4. Nhấn **Run Build** để thực thi build.
5. Kiểm tra file `.bat`, `.log` và kết quả build trong giao diện.
6. Chọn một hoặc nhiều file `.bat` để xem nội dung, chạy riêng hoặc chạy tuần tự.

Có thể mở nhiều tab dự án bằng nút thêm tab. Trạng thái tab và lịch sử dự án được lưu tự động.

## Cấu trúc thư mục

```text
bach_helper/
├── src/                    # Frontend JavaScript và CSS
│   ├── main.js             # Logic giao diện và xử lý tương tác
│   ├── style.css           # CSS tùy chỉnh
│   └── 98.css              # Giao diện Windows 98
├── src-tauri/              # Backend Rust/Tauri
│   ├── src/
│   │   ├── lib.rs          # Khởi tạo app và đăng ký Tauri commands
│   │   └── modules/        # Xử lý build, môi trường, dự án, settings...
│   ├── capabilities/       # Quyền Tauri
│   ├── icons/              # Icon ứng dụng
│   ├── Cargo.toml          # Dependencies Rust
│   └── tauri.conf.json     # Cấu hình cửa sổ và build Tauri
├── public/                 # Tài nguyên public
├── libs/                   # Thư viện/tài nguyên đi kèm
├── dist/                   # Kết quả build frontend
├── index.html              # HTML khởi tạo ứng dụng
├── package.json            # Script npm và dependencies frontend
└── vite.config.js          # Cấu hình Vite
```

## Scripts npm

| Lệnh | Chức năng |
|---|---|
| `npm run dev` | Chạy Vite dev server |
| `npm run build` | Build frontend vào `dist/` |
| `npm run preview` | Preview bản build frontend |
| `npm run tauri dev` | Chạy ứng dụng Tauri ở chế độ phát triển |
| `npm run tauri build` | Build ứng dụng desktop và installer |

## Cấu hình Ant và Java

Ứng dụng có thể:

- Tự động quét Java từ hệ thống.
- Tự động quét Apache Ant trong `PATH` hoặc thư mục `libs`.
- Cho phép chọn JDK hoặc thư mục JDK tùy chỉnh.
- Cho phép chọn Ant hoặc thư mục Ant tùy chỉnh.
- Thiết lập `JAVA_HOME` và `ANT_HOME` nhanh qua giao diện.

Các thiết lập người dùng và cửa sổ được lưu ở file cấu hình do backend quản lý. Có thể xem đường dẫn file cấu hình trong phần Settings của ứng dụng.

## Troubleshooting

### `npm install` lỗi do quyền hoặc cache

Thử xóa cache hoặc cài lại:

```powershell
npm cache verify
npm install
```

Nếu repository đã có `node_modules`, khi phát triển có thể tải lại dependency bằng:

```powershell
Remove-Item -Recurse -Force node_modules
npm ci
```

### Không tìm thấy `java`, `javac` hoặc `ant`

- Kiểm tra JDK/Ant đã được cài đặt.
- Mở Ant/JDK modal trong ứng dụng để quét hoặc chọn thư mục thủ công.
- Kiểm tra biến môi trường:

```powershell
set JAVA_HOME
set ANT_HOME
set PATH
```

- Khởi động lại terminal và ứng dụng sau khi thay đổi biến môi trường.

### Tauri dev không khởi động

Kiểm tra Rust và WebView2:

```powershell
rustup show
winget list Microsoft.EdgeWebView2
```

Cài lại Visual Studio C++ Build Tools nếu Cargo báo thiếu linker hoặc thư viện Windows SDK.

### Cổng `5173` đã được sử dụng

Dừng tiến trình đang dùng cổng hoặc đổi cấu hình trong `vite.config.js` và `tauri.conf.json`, sau đó khởi động lại:

```powershell
npm run tauri dev
```

### Không quét thấy project/target

Dự án cần có:

- Thư mục dự án hợp lệ.
- File `build.xml` trong vị trí mà trình quét có thể tìm thấy.
- Các target Ant hợp lệ trong `build.xml`.

### Build Rust thất bại trên Windows

Đảm bảo đã cài:

- Rust stable
- Microsoft C++ Build Tools
- Windows 10/11 SDK
- WebView2 Runtime

Có thể thử làm sạch build:

```powershell
cd src-tauri
cargo clean
cd ..
npm run tauri dev
```

## Vệ sinh build

Xóa output frontend:

```powershell
Remove-Item -Recurse -Force dist
```

Xóa build Rust:

```powershell
cd src-tauri
cargo clean
```

## Đóng góp

1. Tạo branch mới.
2. Thực hiện thay đổi.
3. Chạy `npm run build` và `npm run tauri dev` để kiểm tra.
4. Tạo Pull Request mô tả rõ thay đổi.

## Giấy phép

Repository hiện chưa khai báo LICENSE hoặc license trong metadata. Nếu dự định phát hành công khai hoặc phân phối ứng dụng, cần thêm file giấy phép phù hợp.

## Tác giả

- GitHub: [QuangThe0412](https://github.com/QuangThe0412)
- Repository: [bach_helper](https://github.com/QuangThe0412/bach_helper)
