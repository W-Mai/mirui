package rs.mirui.android;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.content.Context;
import android.content.res.AssetFileDescriptor;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.Bundle;
import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

/** Read-only URI endpoint registered by the Android host manifest. */
public final class ClipboardProvider extends ContentProvider {
    private static SnapshotStore snapshots;
    private String authority;

    static synchronized SnapshotStore store(Context context) {
        if (snapshots == null) {
            snapshots = new SnapshotStore(new File(context.getNoBackupFilesDir(), "mirui-clipboard"));
        }
        return snapshots;
    }

    static Uri uri(Context context, String token, int item) {
        return new Uri.Builder().scheme("content").authority(context.getPackageName() + ".mirui.clipboard")
                .appendPath(token).appendPath(Integer.toString(item)).build();
    }

    @Override
    public boolean onCreate() {
        Context context = getContext();
        if (context == null) {
            return false;
        }
        authority = context.getPackageName() + ".mirui.clipboard";
        store(context);
        return true;
    }

    private Item item(Uri uri) throws FileNotFoundException {
        List<String> path = uri.getPathSegments();
        if (!"content".equals(uri.getScheme()) || !authority.equals(uri.getAuthority())
                || uri.getQuery() != null || uri.getFragment() != null || path.size() != 2) {
            throw new FileNotFoundException("Invalid clipboard URI");
        }
        try {
            int index = Integer.parseInt(path.get(1));
            if (index < 0 || !Integer.toString(index).equals(path.get(1))) {
                throw new NumberFormatException();
            }
            return new Item(path.get(0), index);
        } catch (NumberFormatException invalid) {
            throw new FileNotFoundException("Invalid clipboard item");
        }
    }

    private static final class Item {
        final String token;
        final int index;

        Item(String token, int index) {
            this.token = token;
            this.index = index;
        }
    }

    @Override
    public String getType(Uri uri) {
        String[] types = getStreamTypes(uri, "*/*");
        return types == null ? null : types[0];
    }

    @Override
    public String[] getStreamTypes(Uri uri, String filter) {
        try {
            Item item = item(uri);
            ArrayList<String> matching = new ArrayList<>();
            for (String type : snapshots.types(item.token, item.index)) {
                if (SnapshotStore.matches(type, filter)) {
                    matching.add(type);
                }
            }
            return matching.isEmpty() ? null : matching.toArray(new String[0]);
        } catch (IOException unavailable) {
            return null;
        }
    }

    @Override
    public AssetFileDescriptor openTypedAssetFile(Uri uri, String filter, Bundle options,
            CancellationSignal cancellation) throws FileNotFoundException {
        if (cancellation != null) {
            cancellation.throwIfCanceled();
        }
        Item item = item(uri);
        try (FileInputStream file = snapshots.open(item.token, item.index, filter)) {
            long size = file.getChannel().size();
            return new AssetFileDescriptor(ParcelFileDescriptor.dup(file.getFD()), 0, size);
        } catch (IOException unavailable) {
            FileNotFoundException failure = new FileNotFoundException("Clipboard data is unavailable");
            failure.initCause(unavailable);
            throw failure;
        }
    }

    @Override
    public AssetFileDescriptor openTypedAssetFile(Uri uri, String filter, Bundle options)
            throws FileNotFoundException {
        return openTypedAssetFile(uri, filter, options, null);
    }

    @Override
    public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!"r".equals(mode)) {
            throw new FileNotFoundException("Clipboard data is read-only");
        }
        return openTypedAssetFile(uri, "*/*", null).getParcelFileDescriptor();
    }

    @Override
    public Cursor query(Uri uri, String[] projection, String selection, String[] arguments, String sortOrder) {
        String[] columns = projection == null
                ? new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE} : projection;
        MatrixCursor result = new MatrixCursor(columns, 1);
        try (AssetFileDescriptor file = openTypedAssetFile(uri, "*/*", null)) {
            Object[] row = new Object[columns.length];
            for (int i = 0; i < columns.length; i++) {
                if (OpenableColumns.DISPLAY_NAME.equals(columns[i])) {
                    row[i] = "clipboard-" + item(uri).index;
                } else if (OpenableColumns.SIZE.equals(columns[i])) {
                    row[i] = file.getLength();
                }
            }
            result.addRow(row);
        } catch (IOException unavailable) {
            // An expired URI has no row.
        }
        return result;
    }

    @Override
    public Uri insert(Uri uri, ContentValues values) {
        throw new UnsupportedOperationException("Clipboard data is read-only");
    }

    @Override
    public int update(Uri uri, ContentValues values, String selection, String[] arguments) {
        throw new UnsupportedOperationException("Clipboard data is read-only");
    }

    @Override
    public int delete(Uri uri, String selection, String[] arguments) {
        throw new UnsupportedOperationException("Clipboard data is read-only");
    }
}
