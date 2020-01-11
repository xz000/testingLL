using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class GetKeyScript : MonoBehaviour
{
    public string MyPrefName;
    //public string MyDefaultValue;
    // Start is called before the first frame update
    public string GetName()
    {
        return PlayerPrefs.GetString(MyPrefName, MyPrefName.Substring(MyPrefName.Length - 1, 1));
    }

    public void SetText()
    {
        GetComponentInChildren<Text>().text = GetName();
    }

    private void Start()
    {
        SetText();
    }
}
