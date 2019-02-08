using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using Steamworks;

public class LanguageSet : MonoBehaviour
{
    public static string CurrentLanguage;
    //public GameObject CV2;

    void Awake()
    {
        SteamAPI.Init();
        CurrentLanguage = SteamApps.GetCurrentGameLanguage();
        //CV2.SetActive(true);
    }
}
