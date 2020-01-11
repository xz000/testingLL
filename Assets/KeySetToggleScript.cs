using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class KeySetToggleScript : MonoBehaviour
{
    public GameObject SetPanel;
    public GetKeyScript[] getKeyScripts;
    // Start is called before the first frame update
    void Start()
    {
        GetComponent<Toggle>().onValueChanged.AddListener(ClickSwitch);
    }

    void ClickSwitch(bool a)
    {
        SetPanel.SetActive(a);
    }

    public void FindExistAndHandle(string existed, string newname)
    {
        foreach (GetKeyScript script in getKeyScripts)
        {
            if (script.GetName() == existed)
            {
                script.GetComponent<SetKeyScript>().SetPref(newname);
            }
        }
    }
}
