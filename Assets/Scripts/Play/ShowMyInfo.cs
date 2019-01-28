using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class ShowMyInfo : MonoBehaviour
{
    public GameObject MyInfoText;
    GameObject MyInfo;
    Rigidbody2D rb2d;

    // Use this for initialization
    void Start()
    {
        GameObject TheCanvas = GameObject.Find("Canvas2");
        MyInfo = GameObject.Instantiate(MyInfoText, TheCanvas.transform);
        rb2d = GetComponent<Rigidbody2D>();
    }

    private void Update()
    {
        MyInfo.transform.position = Camera.main.WorldToScreenPoint(transform.position + Vector3.down * 0.2f);
        MyInfo.GetComponent<Text>().text = rb2d.position.x + "," + rb2d.position.y;
    }

    private void OnDestroy()
    {
        Destroy(MyInfo);
    }
}
