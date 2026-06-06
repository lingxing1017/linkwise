from flask import Flask, Response, jsonify, request, send_from_directory
from html import escape
import base64
import hashlib
import sqlite3
import os
import time
from urllib.parse import urlparse

app = Flask(__name__, static_folder='static', static_url_path='/static')

DB_DIR = 'data'
DB_FILE = os.path.join(DB_DIR, 'bookmarks.db')
DEFAULT_WEBDAV_FILENAME = 'linkwise-bookmarks.html'
DEFAULT_SECRET_FILE = '/run/secrets/linkwise_secret_key'
PASSWORD_AAD = b'linkwise-webdav-password-v1'


def init_db():
    if not os.path.exists(DB_DIR):
        os.makedirs(DB_DIR)

    with sqlite3.connect(DB_FILE) as conn:
        conn.execute('''
            CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY,
                title TEXT,
                url TEXT,
                folder TEXT DEFAULT ''
            )
        ''')

        # 兼容旧数据库：如果旧表没有 folder 字段，就自动加上
        cursor = conn.execute("PRAGMA table_info(bookmarks)")
        columns = [row[1] for row in cursor.fetchall()]

        if 'folder' not in columns:
            conn.execute("ALTER TABLE bookmarks ADD COLUMN folder TEXT DEFAULT ''")

        conn.execute('''
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT DEFAULT ''
            )
        ''')

        conn.commit()


def normalize_url(url):
    url = (url or '').strip()

    if not url:
        return ''

    if url.startswith(('http://', 'https://')):
        return url

    if url.startswith(('chrome://', 'edge://', 'about:', 'javascript:', 'place:')):
        return ''

    # 手动添加 google.com 这种，也自动补 https://
    return 'https://' + url


def is_valid_url(url):
    try:
        parsed = urlparse(url)
        return parsed.scheme in ('http', 'https') and bool(parsed.netloc)
    except Exception:
        return False


def split_folder_path(folder):
    return [
        part.strip()
        for part in str(folder or '').split('/')
        if part.strip()
    ]


def get_all_bookmarks():
    with sqlite3.connect(DB_FILE) as conn:
        cursor = conn.cursor()
        cursor.execute('SELECT id, title, url, folder FROM bookmarks ORDER BY folder ASC, rowid DESC')
        rows = cursor.fetchall()

    return [
        {
            'id': r[0],
            'title': r[1] or '',
            'url': r[2] or '',
            'folder': r[3] or ''
        }
        for r in rows
    ]


def create_export_node(name=''):
    return {
        'name': name,
        'bookmarks': [],
        'children': {}
    }


def build_export_tree(bookmarks):
    root = create_export_node()

    for bookmark in bookmarks:
        current = root

        for part in split_folder_path(bookmark.get('folder')):
            if part not in current['children']:
                current['children'][part] = create_export_node(part)

            current = current['children'][part]

        current['bookmarks'].append(bookmark)

    return root


def render_export_bookmark(bookmark, timestamp, indent):
    title = escape(bookmark.get('title') or '未命名书签')
    url = escape(bookmark.get('url') or '', quote=True)

    return f'{indent}<DT><A HREF="{url}" ADD_DATE="{timestamp}">{title}</A>'


def render_export_node(node, timestamp, depth=1):
    indent = '    ' * depth
    lines = []
    children = sorted(
        node['children'].values(),
        key=lambda child: child['name']
    )

    for child in children:
        folder_name = escape(child['name'])
        lines.append(f'{indent}<DT><H3 ADD_DATE="{timestamp}" LAST_MODIFIED="{timestamp}">{folder_name}</H3>')
        lines.append(f'{indent}<DL><p>')
        lines.extend(render_export_node(child, timestamp, depth + 1))
        lines.append(f'{indent}</DL><p>')

    for bookmark in node['bookmarks']:
        lines.append(render_export_bookmark(bookmark, timestamp, indent))

    return lines


def build_bookmarks_html(bookmarks):
    timestamp = int(time.time())
    tree = build_export_tree(bookmarks)
    lines = [
        '<!DOCTYPE NETSCAPE-Bookmark-file-1>',
        '<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">',
        '<TITLE>Bookmarks</TITLE>',
        '<H1>Bookmarks</H1>',
        '<DL><p>',
        *render_export_node(tree, timestamp),
        '</DL><p>'
    ]

    return '\n'.join(lines)


def get_settings(keys):
    placeholders = ','.join('?' for _ in keys)

    with sqlite3.connect(DB_FILE) as conn:
        rows = conn.execute(
            f'SELECT key, value FROM settings WHERE key IN ({placeholders})',
            tuple(keys)
        ).fetchall()

    return {key: value or '' for key, value in rows}


def save_settings(values):
    with sqlite3.connect(DB_FILE) as conn:
        conn.executemany(
            '''
            INSERT OR REPLACE INTO settings (key, value)
            VALUES (?, ?)
            ''',
            [(key, value) for key, value in values.items()]
        )
        conn.commit()


def delete_settings(keys):
    if not keys:
        return

    placeholders = ','.join('?' for _ in keys)

    with sqlite3.connect(DB_FILE) as conn:
        conn.execute(
            f'DELETE FROM settings WHERE key IN ({placeholders})',
            tuple(keys)
        )
        conn.commit()


def get_linkwise_secret():
    if not os.path.exists(DEFAULT_SECRET_FILE):
        print(f'Linkwise secret file missing: {DEFAULT_SECRET_FILE}', flush=True)
        raise RuntimeError('未配置 linkwise_secret_key，无法保存 WebDAV 密码。')

    with open(DEFAULT_SECRET_FILE, 'r', encoding='utf-8') as file:
        secret = file.read().strip()

    if not secret:
        print(f'Linkwise secret file is empty: {DEFAULT_SECRET_FILE}', flush=True)
        raise RuntimeError('linkwise_secret_key 为空，无法保存 WebDAV 密码。')

    return secret


def has_linkwise_secret():
    try:
        return bool(get_linkwise_secret())
    except RuntimeError:
        return False


def get_password_crypto():
    try:
        from cryptography.hazmat.primitives.ciphers.aead import AESGCM
    except ImportError as exc:
        raise RuntimeError('缺少 cryptography 依赖，无法加密保存 WebDAV 密码。') from exc

    secret = get_linkwise_secret()
    key = hashlib.sha256(secret.encode('utf-8')).digest()
    return AESGCM(key)


def encrypt_webdav_password(password):
    aesgcm = get_password_crypto()
    nonce = os.urandom(12)
    ciphertext = aesgcm.encrypt(nonce, password.encode('utf-8'), PASSWORD_AAD)

    return {
        'webdav_password_ciphertext': base64.b64encode(ciphertext).decode('ascii'),
        'webdav_password_nonce': base64.b64encode(nonce).decode('ascii')
    }


def decrypt_webdav_password(settings):
    ciphertext = settings.get('webdav_password_ciphertext') or ''
    nonce = settings.get('webdav_password_nonce') or ''

    if ciphertext and nonce:
        aesgcm = get_password_crypto()
        decrypted = aesgcm.decrypt(
            base64.b64decode(nonce),
            base64.b64decode(ciphertext),
            PASSWORD_AAD
        )
        return decrypted.decode('utf-8')

    # 兼容之前已经写入的明文配置；重新保存密码后会自动改为密文。
    return settings.get('webdav_password') or ''


def get_webdav_config():
    settings = get_settings([
        'webdav_url',
        'webdav_username',
        'webdav_password',
        'webdav_password_ciphertext',
        'webdav_password_nonce',
        'webdav_remote_dir',
        'webdav_filename'
    ])
    has_encrypted_password = bool(
        settings.get('webdav_password_ciphertext') and
        settings.get('webdav_password_nonce')
    )
    has_legacy_password = bool(settings.get('webdav_password'))

    return {
        'webdav_url': settings.get('webdav_url', ''),
        'username': settings.get('webdav_username', ''),
        'remote_dir': settings.get('webdav_remote_dir', ''),
        'filename': settings.get('webdav_filename') or DEFAULT_WEBDAV_FILENAME,
        'has_password': has_encrypted_password or has_legacy_password,
        'password_security': 'encrypted' if has_encrypted_password else 'legacy_plaintext' if has_legacy_password else 'empty'
    }


@app.route('/')
def index():
    return send_from_directory('.', 'index.html')


@app.route('/api/bookmarks', methods=['GET'])
def get_bookmarks():
    try:
        return jsonify(get_all_bookmarks())

    except Exception as e:
        print('GET /api/bookmarks error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks/export', methods=['GET'])
def export_bookmarks():
    try:
        html = build_bookmarks_html(get_all_bookmarks())
        filename = time.strftime('linkwise-bookmarks-%Y-%m-%d.html')

        return Response(
            html,
            mimetype='text/html; charset=utf-8',
            headers={
                'Content-Disposition': f'attachment; filename="{filename}"'
            }
        )

    except Exception as e:
        print('GET /api/bookmarks/export error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/webdav/config', methods=['GET'])
def read_webdav_config():
    try:
        return jsonify({
            'status': 'success',
            'config': get_webdav_config()
        })

    except Exception as e:
        print('GET /api/webdav/config error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/webdav/config', methods=['POST'])
def update_webdav_config():
    try:
        data = request.get_json(silent=True) or {}
        filename = str(data.get('filename') or DEFAULT_WEBDAV_FILENAME).strip() or DEFAULT_WEBDAV_FILENAME
        password = str(data.get('password') or '')
        current_settings = get_settings([
            'webdav_password',
            'webdav_password_ciphertext',
            'webdav_password_nonce'
        ])

        values = {
            'webdav_url': str(data.get('webdav_url') or '').strip(),
            'webdav_username': str(data.get('username') or '').strip(),
            'webdav_remote_dir': str(data.get('remote_dir') or '').strip(),
            'webdav_filename': filename
        }

        if password:
            values.update(encrypt_webdav_password(password))
        elif current_settings.get('webdav_password') and has_linkwise_secret():
            values.update(encrypt_webdav_password(current_settings.get('webdav_password')))

        save_settings(values)

        if values.get('webdav_password_ciphertext'):
            delete_settings(['webdav_password'])

        return jsonify({
            'status': 'success',
            'config': get_webdav_config()
        })

    except Exception as e:
        print('POST /api/webdav/config error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks', methods=['POST'])
def save_bookmark():
    try:
        data = request.get_json(silent=True) or {}

        print('POST /api/bookmarks received:', data, flush=True)

        b_id = str(data.get('id') or int(time.time() * 1000))
        title = str(data.get('title') or '').strip()
        url = normalize_url(data.get('url'))
        folder = str(data.get('folder') or '').strip()

        if not title:
            return jsonify({'status': 'error', 'message': '标题不能为空'}), 400

        if not is_valid_url(url):
            return jsonify({'status': 'error', 'message': 'URL 无效'}), 400

        with sqlite3.connect(DB_FILE) as conn:
            duplicate = conn.execute(
                '''
                SELECT id, title, url, folder
                FROM bookmarks
                WHERE url = ? AND id != ?
                LIMIT 1
                ''',
                (url, b_id)
            ).fetchone()

            if duplicate:
                return jsonify({
                    'status': 'duplicate',
                    'message': '这个 URL 已存在',
                    'bookmark': {
                        'id': duplicate[0],
                        'title': duplicate[1],
                        'url': duplicate[2],
                        'folder': duplicate[3] or ''
                    }
                }), 409

            conn.execute(
                'INSERT OR REPLACE INTO bookmarks (id, title, url, folder) VALUES (?, ?, ?, ?)',
                (b_id, title, url, folder)
            )
            conn.commit()

            count = conn.execute('SELECT COUNT(*) FROM bookmarks').fetchone()[0]

        return jsonify({
            'status': 'success',
            'id': b_id,
            'title': title,
            'url': url,
            'folder': folder,
            'total_count': count
        })

    except Exception as e:
        print('POST /api/bookmarks error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks/bulk', methods=['POST'])
def bulk_save_bookmarks():
    try:
        data = request.get_json(silent=True) or {}
        items = data.get('bookmarks', [])

        if not isinstance(items, list):
            return jsonify({'status': 'error', 'message': '导入数据格式不正确'}), 400

        valid_items = []
        skipped = 0
        duplicate_count = 0
        seen_urls = set()
        now = int(time.time() * 1000)

        with sqlite3.connect(DB_FILE) as conn:
            existing_urls = {
                row[0]
                for row in conn.execute('SELECT url FROM bookmarks WHERE url IS NOT NULL')
                if row[0]
            }

            for index, item in enumerate(items):
                if not isinstance(item, dict):
                    skipped += 1
                    continue

                b_id = str(item.get('id') or f'{now}-{index}')
                title = str(item.get('title') or '未命名书签').strip()
                url = normalize_url(item.get('url'))
                folder = str(item.get('folder') or '').strip()

                if not title or not is_valid_url(url):
                    skipped += 1
                    continue

                if url in existing_urls or url in seen_urls:
                    duplicate_count += 1
                    continue

                seen_urls.add(url)
                valid_items.append((b_id, title, url, folder))

            if valid_items:
                conn.executemany(
                    '''
                    INSERT OR REPLACE INTO bookmarks (id, title, url, folder)
                    VALUES (?, ?, ?, ?)
                    ''',
                    valid_items
                )
                conn.commit()

            total_count = conn.execute('SELECT COUNT(*) FROM bookmarks').fetchone()[0]

        return jsonify({
            'status': 'success',
            'imported_count': len(valid_items),
            'imported_ids': [item[0] for item in valid_items],
            'duplicate_count': duplicate_count,
            'skipped_count': skipped,
            'total_count': total_count
        })

    except Exception as e:
        print('POST /api/bookmarks/bulk error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks/move', methods=['POST'])
def move_bookmarks():
    try:
        data = request.get_json(silent=True) or {}
        ids = data.get('ids', [])
        folder = ' / '.join(
            part.strip()
            for part in str(data.get('folder') or '').split('/')
            if part.strip()
        )

        if not isinstance(ids, list) or not ids:
            return jsonify({'status': 'error', 'message': '请选择要移动的书签'}), 400

        ids = [str(item).strip() for item in ids if str(item).strip()]

        if not ids:
            return jsonify({'status': 'error', 'message': '请选择要移动的书签'}), 400

        placeholders = ','.join('?' for _ in ids)

        with sqlite3.connect(DB_FILE) as conn:
            cursor = conn.execute(
                f'UPDATE bookmarks SET folder = ? WHERE id IN ({placeholders})',
                [folder, *ids]
            )
            conn.commit()

        return jsonify({
            'status': 'success',
            'moved_count': cursor.rowcount,
            'folder': folder
        })

    except Exception as e:
        print('POST /api/bookmarks/move error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks/delete', methods=['POST'])
def delete_bookmarks():
    try:
        data = request.get_json(silent=True) or {}
        ids = data.get('ids', [])

        if not isinstance(ids, list) or not ids:
            return jsonify({'status': 'error', 'message': '请选择要删除的书签'}), 400

        ids = [str(item).strip() for item in ids if str(item).strip()]

        if not ids:
            return jsonify({'status': 'error', 'message': '请选择要删除的书签'}), 400

        placeholders = ','.join('?' for _ in ids)

        with sqlite3.connect(DB_FILE) as conn:
            cursor = conn.execute(
                f'DELETE FROM bookmarks WHERE id IN ({placeholders})',
                ids
            )
            conn.commit()

        return jsonify({
            'status': 'success',
            'deleted_count': cursor.rowcount
        })

    except Exception as e:
        print('POST /api/bookmarks/delete error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/bookmarks/<b_id>', methods=['DELETE'])
def delete_bookmark(b_id):
    try:
        with sqlite3.connect(DB_FILE) as conn:
            conn.execute('DELETE FROM bookmarks WHERE id = ?', (b_id,))
            conn.commit()

        return jsonify({'status': 'success'})

    except Exception as e:
        print('DELETE /api/bookmarks error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/folders/move-up', methods=['POST'])
def move_folder_bookmarks_up():
    try:
        data = request.get_json(silent=True) or {}
        folder = str(data.get('folder') or '').strip()

        if not folder:
            return jsonify({'status': 'error', 'message': '请选择要操作的目录'}), 400

        parts = [p.strip() for p in folder.split('/') if p.strip()]
        parent_folder = ' / '.join(parts[:-1])

        with sqlite3.connect(DB_FILE) as conn:
            rows = conn.execute(
                'SELECT id, folder FROM bookmarks WHERE folder = ? OR folder LIKE ?',
                (folder, folder + ' / %')
            ).fetchall()

            moved_count = 0

            for b_id, old_folder in rows:
                old_folder = old_folder or ''

                if old_folder == folder:
                    new_folder = parent_folder
                elif old_folder.startswith(folder + ' / '):
                    suffix = old_folder[len(folder + ' / '):]
                    new_folder = f'{parent_folder} / {suffix}' if parent_folder else suffix
                else:
                    continue

                conn.execute(
                    'UPDATE bookmarks SET folder = ? WHERE id = ?',
                    (new_folder, b_id)
                )
                moved_count += 1

            conn.commit()

        return jsonify({
            'status': 'success',
            'message': '目录已删除，书签已移动到上一层',
            'moved_count': moved_count,
            'parent_folder': parent_folder
        })

    except Exception as e:
        print('POST /api/folders/move-up error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/folders/rename', methods=['POST'])
def rename_folder():
    try:
        data = request.get_json(silent=True) or {}
        folder = str(data.get('folder') or '').strip()
        new_folder = ' / '.join(
            part.strip()
            for part in str(data.get('new_folder') or '').split('/')
            if part.strip()
        )

        if not folder:
            return jsonify({'status': 'error', 'message': '请选择要操作的目录'}), 400

        if folder == new_folder:
            return jsonify({'status': 'error', 'message': '新目录和原目录相同'}), 400

        with sqlite3.connect(DB_FILE) as conn:
            rows = conn.execute(
                'SELECT id, folder FROM bookmarks WHERE folder = ? OR folder LIKE ?',
                (folder, folder + ' / %')
            ).fetchall()

            renamed_count = 0

            for b_id, old_folder in rows:
                old_folder = old_folder or ''

                if old_folder == folder:
                    updated_folder = new_folder
                elif old_folder.startswith(folder + ' / '):
                    suffix = old_folder[len(folder + ' / '):]
                    updated_folder = f'{new_folder} / {suffix}' if new_folder else suffix
                else:
                    continue

                conn.execute(
                    'UPDATE bookmarks SET folder = ? WHERE id = ?',
                    (updated_folder, b_id)
                )
                renamed_count += 1

            conn.commit()

        return jsonify({
            'status': 'success',
            'message': '目录已更新',
            'renamed_count': renamed_count,
            'folder': folder,
            'new_folder': new_folder
        })

    except Exception as e:
        print('POST /api/folders/rename error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


@app.route('/api/folders/delete', methods=['POST'])
def delete_folder_with_bookmarks():
    try:
        data = request.get_json(silent=True) or {}
        folder = str(data.get('folder') or '').strip()

        if not folder:
            return jsonify({'status': 'error', 'message': '请选择要操作的目录'}), 400

        with sqlite3.connect(DB_FILE) as conn:
            cursor = conn.execute(
                'DELETE FROM bookmarks WHERE folder = ? OR folder LIKE ?',
                (folder, folder + ' / %')
            )
            conn.commit()

        return jsonify({
            'status': 'success',
            'message': '目录和目录下书签已删除',
            'deleted_count': cursor.rowcount
        })

    except Exception as e:
        print('POST /api/folders/delete error:', e, flush=True)
        return jsonify({'status': 'error', 'message': str(e)}), 500


init_db()

if __name__ == '__main__':
    app.run(host='0.0.0.0', port=7500)
