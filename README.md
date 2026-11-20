# Batch Helper

Ứng dụng desktop hỗ trợ quản lý và build dự án Java sử dụng Apache Ant.

## Công nghệ

- **Framework:** Tauri 2, Vite
- **Ngôn ngữ:** Rust, JavaScript, HTML, CSS

## Chạy phát triển

Cài dependencies:

```bash
npm install
```

Khởi chạy ứng dụng desktop:

```bash
npm run tauri dev
```

## Build ứng dụng

Tạo bản build desktop:

```bash
npm run tauri build
```

Chỉ build frontend:

```bash
npm run build
```

## Tính năng chính

- Quản lý nhiều dự án Java bằng các tab.
- Quét và chạy target Apache Ant.
- Xem, lọc và chạy file `.bat`, hỗ trợ chạy theo nhóm.
- Kiểm tra cấu hình Java/JDK, Ant và database.
- Theo dõi log build và tùy chỉnh cài đặt screenshot.
