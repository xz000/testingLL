using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using Steamworks;

public class UserListScript : MonoBehaviour
{
    public GameObject Udetail;
    public GameObject[] UDs;
    public Transform Father;

    public void CreateDs(int a)
    {
        DestroyDs();
        UDs = new GameObject[a];
        for (int i = 0; i < a; i++)
            UDs[i] = Instantiate(Udetail, Father);
    }

    public void DestroyDs()
    {
        if (UDs == null)
            return;
        for (int i = 0; i < UDs.Length; i++)
            Destroy(UDs[i]);
    }

    public static Texture2D GetSteamImageAsTexture2D(int iImage)
    {
        Texture2D ret = null;
        uint ImageWidth;
        uint ImageHeight;
        bool bIsValid = SteamUtils.GetImageSize(iImage, out ImageWidth, out ImageHeight);

        if (bIsValid)
        {
            byte[] Image = new byte[ImageWidth * ImageHeight * 4];

            bIsValid = SteamUtils.GetImageRGBA(iImage, Image, (int)(ImageWidth * ImageHeight * 4));
            if (bIsValid)
            {
                ret = new Texture2D((int)ImageWidth, (int)ImageHeight, TextureFormat.RGBA32, false, true);
                ret.LoadRawTextureData(Image);
                ret.Apply();
            }
        }
        ret = FlipTexture(ret);
        return ret;
    }

    static Texture2D FlipTexture(Texture2D original)
    {
        if (original == null)
            return null;
        Texture2D flipped = new Texture2D(original.width, original.height);

        int xN = original.width;
        int yN = original.height;

        for (int i = 0; i < xN; i++)
        {
            for (int j = 0; j < yN; j++)
            {
                flipped.SetPixel(i, yN - j - 1, original.GetPixel(i, j));
            }
        }

        flipped.Apply();

        return flipped;
    }
}
