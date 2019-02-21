using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class MultiTextSet : MonoBehaviour
{
    public string TextEn;
    public string TextCn;

    private void Start()
    {
        switch (LanguageSet.CurrentLanguage)
        {
            case "schinese":
                GetComponent<Text>().text = TextCn.Replace("//n", "/n");
                break;
        }
    }
}
