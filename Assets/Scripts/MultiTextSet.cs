using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class MultiTextSet : MonoBehaviour
{
    public string TextEn;
    public string TextCn;

    private void Start()
    {
        switch (SteamApps.GetCurrentGameLanguage())
        {
            case "schinese":
                GetComponent<Text>().text = TextCn.Replace("\\n", "\n");
                break;
        }
    }
}
